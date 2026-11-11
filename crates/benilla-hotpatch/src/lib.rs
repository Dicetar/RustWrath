//! Local trusted modules only. Validation/rollback is not isolation from native memory faults.
//!
//! Two seams are hosted here, each with its own publication and slot pair so neither can disturb
//! the other: the pose postprocessor in `ready.toml` (`pose-<sha1>.dll`) and the optional movement
//! processor in `movement.toml` (`movement-<sha1>.dll`). Both manifests share the same folder and
//! `publish.lock`; a movement candidate is staged without re-reading or invalidating the pose
//! publication, and its validation/handback is independent of the pose module's rollback history.
use anyhow::{bail, ensure, Context, Result};
use benilla_hotpatch_abi::{
    BoneLocal, EntryPoint, Header, ModuleKind, MovementFrame, MovementModuleV1, MovementOutput,
    PoseFrame, ProcessMovement, ProcessPose, RigSpan, ABI_VERSION, ENTRY, MOVEMENT_ENTRY,
    MOVEMENT_MAGIC,
};
use libloading::Library;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::VecDeque,
    io::Write,
    path::{Component, Path, PathBuf},
};

const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;
/// Manifest file naming the optional movement processor, beside the pose `ready.toml`. The pose
/// publication is deliberately left at its own path so a movement candidate can be staged while a
/// pose module stays active, without re-reading or invalidating the pose manifest.
const MOVEMENT_MANIFEST: &str = "movement.toml";
/// Bounded restore-outcome ring. One `evaluate` can report at most the active module plus the
/// three retained predecessors; the cap only bounds an unconsumed producer.
const MAX_RESTORE_OUTCOMES: usize = 8;
/// Bounded quarantine (in-memory plus `quarantine.toml`) of publication digests that failed after their bytes were verified
/// (a content/contract error or an evaluation failure that reached a [`PatchHealthPolicy`]
/// threshold, not transient I/O or a frame rejected by host validation). The digest is the
/// quarantine key, so a changed manifest text that still names the same bad artifact is refused
/// before any DLL read or `Library::new`. The same store covers a post-verification load failure
/// and a loaded processor that is rejected by a canary batch or fails while active; retained copies
/// are dropped or skipped. The pose and movement seams each own a store with this cap. Both stores
/// are persisted by `Host::persist_quarantine` and restored by `Host::load_persistent_quarantine`
/// through `quarantine.toml`; remaining movement/rollback typed reasons and the movement
/// active-frame sentinel remain H03.
const MAX_QUARANTINED_DIGESTS: usize = 32;
/// Digest-only quarantine store beside the publications. It records exactly the two bounded digest
/// lists: no library path, revision, timestamp or OS error text is written, and no entry is ever
/// joined to a path or opened as a file. It is not a manifest and never selects code to load.
const QUARANTINE_FILE: &str = "quarantine.toml";
/// Temp sibling used for the write-then-rename replacement of [`QUARANTINE_FILE`].
const QUARANTINE_TEMP: &str = "quarantine.toml.tmp";
/// Upper bound on the digest-only quarantine file. The file holds at most 64 digests, so this is
/// generous; a larger file is rejected before it is read into memory.
const MAX_QUARANTINE_FILE_BYTES: u64 = 64 * 1024;
/// Pose/movement crash sentinel beside the publications: written after the artifact bytes are
/// verified and before the irreversible native call (`Library::new` for the loading windows, the
/// staged pose candidate's `process` for the canary window), so a native fault in a call leaves a
/// marker the next launch can act on. Cleared again once the call returns.
const ACTIVE_SENTINEL_FILE: &str = "active-sentinel.toml";
/// Temp sibling used for the write-then-rename replacement of [`ACTIVE_SENTINEL_FILE`].
const ACTIVE_SENTINEL_TEMP: &str = "active-sentinel.toml.tmp";
/// Upper bound on the digest-only crash marker. The file holds four short fields; a larger file
/// is rejected before it is read into memory.
const MAX_ACTIVE_SENTINEL_BYTES: u64 = 16 * 1024;
/// Bounded persisted state beside the publications: the demotion policy, per-revision cumulative
/// health for the active pose and movement modules, and the two artifact stamps that decide whether
/// an identical-text manifest latches a terminal failure. It is not a manifest and never selects
/// code to load: a persisted digest only seeds counters for a later verified activation, and a
/// stamp only says which artifact metadata a latched publication last observed. It is
/// single-client like `quarantine.toml` (fixed temp name, last-writer-wins replacement).
const PATCH_STATE_FILE: &str = "patch-state.toml";
/// Temp sibling used for the write-then-rename replacement of [`PATCH_STATE_FILE`].
const PATCH_STATE_TEMP: &str = "patch-state.toml.tmp";
/// Upper bound on the whole state document. Two bounded stores (`MAX_PERSISTED_REVISIONS` entries
/// each) plus the policy and two stamps fit comfortably; a larger file is rejected before it is
/// read.
const MAX_PATCH_STATE_BYTES: u64 = 64 * 1024;
/// Bound on the persisted per-revision health entries. The digest is the key; the newest entry is
/// last and the oldest is evicted at the cap.
const MAX_PERSISTED_REVISIONS: usize = 16;
/// Bound on a persisted revision label. Empty, longer or control-containing labels are skipped as
/// invalid entries rather than treated as a document error.
const MAX_PERSISTED_REVISION_BYTES: usize = 128;
/// Bound on the latched manifest text stored beside a terminal-failure artifact stamp. The text is
/// the exact `ready.toml`/`movement.toml` content needed to make the stamp useful after a restart;
/// a larger document is dropped rather than persisted partially.
const MAX_PERSISTED_LATCH_BYTES: usize = 4096;
/// Only schema version this binary understands. A different version is rejected wholesale so a
/// newer file is never partially applied.
const PATCH_STATE_SCHEMA_VERSION: u32 = 1;

/// Verified SHA-1 content identity of a loaded module, as raw bytes.
pub type ModuleDigest = [u8; 20];

/// Recorder-safe reason for one pose evaluation failure.
///
/// The free-form loader/evaluation detail stays in `take_rejection`/`take_active_failure` and the
/// local log; this enum is the stable code carried in recorder events and restore outcomes.
/// `Status` describes a module-produced non-zero status, while `InvalidOutput` and
/// `TopologyChanged` split the module output checks (an invalid transform is reported before a
/// changed `BoneLocal.parent` when both are wrong). `FrameTooLarge` is the `Loaded::evaluate`
/// ABI-count guard, and `HostFrameRejected` is the adapter's host/caller frame `Err` branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatchEvaluationReason {
    /// The module returned this non-zero status for the pose frame.
    Status(i32),
    /// The module returned a non-finite transform, a zero-norm rotation or another invalid output.
    InvalidOutput,
    /// The module changed `BoneLocal.parent`; when the transform is also invalid,
    /// [`Self::InvalidOutput`] wins because transform validity is checked first.
    TopologyChanged,
    /// The `Loaded::evaluate` ABI-count guard rejected the frame before dispatch.
    FrameTooLarge,
    /// The adapter's `Host::evaluate` `Err` branch: the host/caller frame was rejected.
    HostFrameRejected,
    /// Fallback for an untyped or future evaluation failure.
    Unknown,
}

/// Recorder-safe description of one active-module frame failure and its fallback decision.
///
/// Free-form loader errors stay in `take_active_failure`/logs. This enum carries the verified
/// content identity (when available), whether a retained predecessor was promoted, and the last
/// evaluation reason that made a restore unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// The failed module was replaced by the newest retained predecessor.
    Restored {
        failed_digest: Option<ModuleDigest>,
        /// `None` only when the restored module's identity could not be encoded.
        restored_digest: Option<ModuleDigest>,
    },
    /// The failed module had no retained predecessor; the baseline pose is active again.
    Unavailable {
        failed_digest: Option<ModuleDigest>,
        /// The last predecessor's evaluation reason, or the active module's own reason when no
        /// predecessor was tried.
        reason: PatchEvaluationReason,
    },
}

/// Read-only health snapshot of the currently active pose module.
///
/// The counters and timings describe the active module's steady-state evaluations; the activation
/// probe that promoted it (candidate canary or predecessor restore) is untimed, so both timings are
/// 0 until the first post-activation frame. `revision`/`digest` name the module active at snapshot
/// time; after a demotion with no valid predecessor the slot is empty, so both are `None` while the
/// last counters/timings remain visible until the next activation resets them (the failed identity
/// is still available from `take_active_failure`/`take_restore_outcomes`). A restored predecessor
/// is a new activation and starts a clean consecutive streak, but its cumulative failures and
/// high-water evaluation time are seeded from the bounded per-revision store in
/// `patch-state.toml` when that digest was seen in an earlier activation or process run.
/// [`PatchHealthPolicy`] enforces the thresholds in memory. Remaining movement typed reasons
/// remain H03 (movement health is host-current and persisted; see [`MovementHealthSnapshot`]); the
/// persisted store is described on
/// [`Host::load_persistent_state`]/[`Host::persist_state`]. (The separate digest quarantine behind
/// `quarantined_digests` is persisted too, in `quarantine.toml`.)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PatchHealthSnapshot {
    /// Revision of the module active at snapshot time, or `None` when the slot is empty.
    pub revision: Option<String>,
    /// Verified SHA-1 of the module active at snapshot time, or `None` when the slot is empty.
    pub digest: Option<String>,
    /// Active-module frame failures since its last success or activation. This is the per-
    /// activation streak, not the cumulative count.
    pub consecutive_frame_failures: u32,
    /// Frame failures recorded for this verified digest since it was first seen, cumulative across
    /// reactivations and process runs. This is the count
    /// [`PatchHealthPolicy::max_total_frame_failures`] compares; a reactivation seeds it from
    /// `patch-state.toml`, while `consecutive_frame_failures` starts at 0 for the new activation.
    pub total_frame_failures: u32,
    /// Duration of the most recent steady-state active-module evaluation, in microseconds. The
    /// activation probe that promoted this module (candidate canary or predecessor restore) is not
    /// timed, so this is 0 until the first post-activation frame.
    pub last_eval_micros: u64,
    /// High-water mark of steady-state active-module evaluation duration, in microseconds. Seeded
    /// from the per-revision store on reactivation.
    pub max_eval_micros: u64,
    /// Number of pose digests currently held in the bounded quarantine. The list is loaded from
    /// and persisted to `quarantine.toml`; this count is a live in-memory length.
    pub quarantined_digests: usize,
}

/// Demotion thresholds for the active pose module.
///
/// Both thresholds count the active module's failed frames, not candidate canary failures or
/// predecessor restore probes. The consecutive threshold restarts on every activation (candidate
/// promotion or a successful predecessor restore); the total threshold is per-revision cumulative
/// across reactivations and process runs, seeded from `patch-state.toml`. The thresholds are
/// independent alternatives: a failure demotes as soon as *either* enabled threshold is reached.
///
/// A value of `0` disables that threshold, meaning "no limit" rather than "demote on the first
/// failure": `max_consecutive_frame_failures = 0` ignores the consecutive streak and
/// `max_total_frame_failures = 0` ignores the per-revision cumulative count. `0/0` therefore
/// never demotes a module on health counters alone; only a caller can change the policy or the
/// host can lose the module through another failure path.
///
/// The default [`PatchHealthPolicy::default`] is the original single-strike behavior: one
/// consecutive failure demotes, with the cumulative cap (3) only reachable if the consecutive
/// threshold were relaxed. The policy is loaded from and saved to `patch-state.toml`; remaining
/// movement/rollback evaluation reasons and `max_eval_micros` enforcement remain H03 work (movement
/// health has its own live [`MovementHealthPolicy`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatchHealthPolicy {
    /// Consecutive active-frame failures that demote the module. `0` disables this threshold.
    pub max_consecutive_frame_failures: u32,
    /// Per-revision cumulative active-frame failures that demote the module. `0` disables this
    /// threshold. The count is seeded across reactivations from `patch-state.toml`.
    pub max_total_frame_failures: u32,
}

impl Default for PatchHealthPolicy {
    fn default() -> Self {
        Self {
            max_consecutive_frame_failures: 1,
            max_total_frame_failures: 3,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    revision: String,
    library: String,
    sha1: String,
}

/// On-disk shape of `<root>/quarantine.toml`: the two bounded digest lists, oldest first.
///
/// This is a digest archive, not a manifest: `deny_unknown_fields` rejects a file carrying any
/// other key, both lists default to empty, and every entry is re-validated on load by
/// [`quarantine_entry_digest`]. Nothing here is trusted as a path, revision or identity; an entry
/// only ever becomes a quarantine key.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuarantineFile {
    #[serde(default)]
    pose: Vec<String>,
    #[serde(default)]
    movement: Vec<String>,
}

/// Which native seam a crash sentinel belongs to. The pose seam covers the loading and staged-canary
/// windows; the movement seam covers its loading and staged-canary windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SentinelKind {
    Pose,
    Movement,
}

/// Lifecycle stage of a crash sentinel. The pose loading/staged-canary and movement
/// loading/staged-canary windows are implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SentinelStage {
    Loading,
    /// Staged-candidate canary window: written immediately before the pose candidate's
    /// `Loaded::evaluate` call or the first staged movement proposal's `process` call, and cleared
    /// when that call returns.
    Canary,
}

/// On-disk shape of `<root>/active-sentinel.toml`: the digest-only marker of the pose or movement
/// module that was being loaded or canary-evaluated when the process died. It carries no library
/// path and no OS error text, so it can never be joined to a path or mistaken for a diagnosis.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActiveSentinelFile {
    revision: String,
    sha1: String,
    kind: SentinelKind,
    stage: SentinelStage,
}

/// On-disk shape of `<root>/patch-state.toml`.
///
/// Only `schema_version` is required. The sections are optional so an older or minimal file stays
/// loadable, but a present `[policy]` table requires both thresholds: a missing field is a document
/// error, never a silent `0`. `deny_unknown_fields` rejects a file that carries any other key,
/// including one written by a newer binary, so the whole document is understood or none of it is
/// applied. `[movement_health]` carries the same bounded per-revision shape as `[health]` for the
/// movement seam and an absent table loads an empty movement store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchStateFile {
    schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy: Option<PersistedPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    health: Option<PersistedHealth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    movement_health: Option<PersistedHealth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stamps: Option<PersistedStamps>,
}

/// Persisted demotion thresholds. Both fields are required when the table is present.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedPolicy {
    max_consecutive_frame_failures: u32,
    max_total_frame_failures: u32,
}

/// Persisted per-revision health, oldest entry first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedHealth {
    #[serde(default)]
    revisions: Vec<PersistedRevisionHealth>,
}

/// One bounded per-revision cumulative health record.
///
/// `total_frame_failures` is cumulative across reactivations and `max_eval_micros` is the steady-
/// state high-water mark; neither is trusted as authorization. The digest is re-validated by
/// [`quarantine_entry_digest`] and only seeds counters for a later verified activation of that
/// exact content identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedRevisionHealth {
    digest: String,
    revision: String,
    total_frame_failures: u32,
    max_eval_micros: u64,
}

/// Persisted artifact stamps, one optional entry per seam.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedStamps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pose: Option<PersistedStamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    movement: Option<PersistedStamp>,
}

/// File metadata of the artifact a latched manifest last named.
///
/// The shape mirrors [`ArtifactStamp`]: `modified_unix_nanos` is `Option<u64>` and converts with
/// `UNIX_EPOCH.checked_add(Duration::from_nanos(..))`, so an out-of-range value becomes `None`
/// (length-only comparison) instead of failing the document. `library` must still be a single
/// normal basename when loaded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedStamp {
    library: String,
    len: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modified_unix_nanos: Option<u64>,
    /// The exact manifest text whose terminal failure recorded this stamp. Restoring it lets the
    /// first poll/stage call take the unchanged-text latched branch and consult the stamp before
    /// re-verifying. Bounded by [`MAX_PERSISTED_LATCH_BYTES`]; a stamp without a valid latch is
    /// omitted because it would be unusable after a restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    latch: Option<String>,
}

/// Accept exactly 40 ASCII hex digits, normalized to lowercase; everything else is skipped.
///
/// The rejection is deliberately broad: empty, short, over-long/overflowing, nested/path-like
/// (`pose-<digest>.dll`, `../`, `a/b`), non-hex and non-ASCII entries all fail here and are never
/// counted, joined to a path or used as a filename. A skipped entry is not an error: the file is a
/// best-effort archive, and one corrupt line must not discard the digests beside it.
fn quarantine_entry_digest(entry: &str) -> Option<String> {
    if entry.len() == 40 && entry.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(entry.to_ascii_lowercase())
    } else {
        None
    }
}

/// Validate one file's list into the digests this host would keep, newest-last.
///
/// Invalid entries are dropped and duplicates collapse to their first position. A list longer than
/// [`MAX_QUARANTINED_DIGESTS`] keeps only its newest entries, the same suffix a fresh merge ends up
/// holding, so re-reading an oversized file inserts nothing the second time instead of rotating the
/// store with its already-evicted prefix. This is shape/cap hygiene, not a security boundary: an
/// accepted entry is only ever a quarantine key compared against a verified digest.
fn accepted_quarantine_digests(entries: &[String]) -> Vec<String> {
    let mut digests: Vec<String> = Vec::new();
    for entry in entries {
        let Some(digest) = quarantine_entry_digest(entry) else {
            continue;
        };
        if digests.contains(&digest) {
            continue;
        }
        digests.push(digest);
    }
    if digests.len() > MAX_QUARANTINED_DIGESTS {
        let keep_from = digests.len() - MAX_QUARANTINED_DIGESTS;
        digests = digests.split_off(keep_from);
    }
    digests
}

/// Whether `library` is a single normal basename inside the publication root.
///
/// This is the same shape check `artifact_stamp` and `verify_publication` use: a persisted stamp
/// never becomes a path that can escape the module folder, and a parsed manifest may name a path
/// only to have it refused before any stat/read.
fn plain_library_basename(library: &str) -> bool {
    let path = Path::new(library);
    path.components().count() == 1 && matches!(path.components().next(), Some(Component::Normal(_)))
}

/// Accept a persisted revision label: non-empty, at most [`MAX_PERSISTED_REVISION_BYTES`] bytes and
/// free of control characters. Everything else is an invalid entry and is skipped silently.
fn accepted_revision_label(revision: &str) -> Option<&str> {
    if revision.is_empty()
        || revision.len() > MAX_PERSISTED_REVISION_BYTES
        || revision.chars().any(char::is_control)
    {
        None
    } else {
        Some(revision)
    }
}

/// Validate one file's per-revision health list into the bounded newest-last window.
///
/// Invalid digests and labels are skipped, a duplicate digest replaces its earlier entry and moves
/// to the newest end, and a list longer than [`MAX_PERSISTED_REVISIONS`] keeps only its newest
/// entries. Loading the same file twice therefore yields the same window. The digest is normalized
/// through [`quarantine_entry_digest`]; the counters are ordinary `u32`/`u64` and grant nothing.
fn accepted_persisted_revisions(
    entries: &[PersistedRevisionHealth],
) -> VecDeque<PersistedRevisionHealth> {
    let mut revisions: VecDeque<PersistedRevisionHealth> = VecDeque::new();
    for entry in entries {
        let Some(digest) = quarantine_entry_digest(&entry.digest) else {
            continue;
        };
        let Some(revision) = accepted_revision_label(&entry.revision) else {
            continue;
        };
        if let Some(index) = revisions.iter().position(|held| held.digest == digest) {
            revisions.remove(index);
        }
        if revisions.len() == MAX_PERSISTED_REVISIONS {
            revisions.pop_front();
        }
        revisions.push_back(PersistedRevisionHealth {
            digest,
            revision: revision.to_owned(),
            total_frame_failures: entry.total_frame_failures,
            max_eval_micros: entry.max_eval_micros,
        });
    }
    revisions
}

/// Convert persisted nanoseconds since the Unix epoch to a [`SystemTime`].
///
/// An out-of-range value is `None`, which [`ArtifactStamp::matches_current`] treats as a
/// length-only comparison rather than a document error.
fn system_time_from_unix_nanos(nanos: u64) -> Option<std::time::SystemTime> {
    std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_nanos(nanos))
}

/// Convert a [`SystemTime`] to nanoseconds since the Unix epoch for the persisted stamp.
///
/// A pre-epoch time, a value wider than `u64` nanoseconds, or a value that does not fit TOML's
/// signed 64-bit integer becomes `None`; the reloaded stamp then compares by length only.
fn unix_nanos_from_system_time(time: std::time::SystemTime) -> Option<u64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| u64::try_from(since.as_nanos()).ok())
        .filter(|nanos| *nanos <= i64::MAX as u64)
}

/// Convert a persisted stamp into the in-memory artifact identity plus its latched manifest text,
/// or skip both when either half is invalid.
fn accepted_persisted_stamp(stamp: PersistedStamp) -> Option<(ArtifactStamp, String)> {
    if !plain_library_basename(&stamp.library) || stamp.len > MAX_LIBRARY_BYTES {
        return None;
    }
    let latch = stamp.latch?;
    if latch.is_empty() || latch.len() > MAX_PERSISTED_LATCH_BYTES {
        return None;
    }
    let publication: Publication = toml::from_str(&latch).ok()?;
    if publication.library != stamp.library
        || publication.revision.is_empty()
        || quarantine_entry_digest(&publication.sha1).is_none()
    {
        return None;
    }
    Some((
        ArtifactStamp {
            library: stamp.library,
            len: stamp.len,
            modified: stamp
                .modified_unix_nanos
                .and_then(system_time_from_unix_nanos),
        },
        latch,
    ))
}

/// Serialize an artifact stamp into its bounded persisted shape together with the latched manifest
/// text, skipping one the loader would reject (a non-basename/oversized library, a missing or
/// oversized latch, or a latch that no longer parses as the same publication).
fn persisted_stamp(stamp: &ArtifactStamp, latch: Option<&str>) -> Option<PersistedStamp> {
    if !plain_library_basename(&stamp.library) || stamp.len > MAX_LIBRARY_BYTES {
        return None;
    }
    let latch = latch?;
    if latch.is_empty() || latch.len() > MAX_PERSISTED_LATCH_BYTES {
        return None;
    }
    let publication: Publication = toml::from_str(latch).ok()?;
    if publication.library != stamp.library
        || publication.revision.is_empty()
        || quarantine_entry_digest(&publication.sha1).is_none()
    {
        return None;
    }
    Some(PersistedStamp {
        library: stamp.library.clone(),
        len: stamp.len,
        modified_unix_nanos: stamp.modified.and_then(unix_nanos_from_system_time),
        latch: Some(latch.to_owned()),
    })
}

/// Write the digest-only store to `temp` and flush it before the caller renames it over the target.
///
/// This mirrors `benilla_app::local_state::write_atomic_bytes`: create the folder, create/truncate
/// the temp file, `write_all`, `sync_all`, then let the caller's `rename` replace the target. A
/// failure at any step leaves the old target untouched, so the caller only has to remove the temp.
fn write_quarantine_temp(root: &Path, temp: &Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(root).context("create publication folder")?;
    let mut file = std::fs::File::create(temp).context("create quarantine temp file")?;
    file.write_all(text.as_bytes())
        .context("write quarantine temp file")?;
    file.sync_all().context("flush quarantine temp file")
}

/// Write the crash-sentinel document to its temp sibling and flush it, mirroring
/// [`write_quarantine_temp`] for the `active-sentinel.toml` document shape.
fn write_sentinel_temp(root: &Path, temp: &Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(root).context("create publication folder")?;
    let mut file = std::fs::File::create(temp).context("create sentinel temp file")?;
    file.write_all(text.as_bytes()).context("write sentinel temp file")?;
    file.sync_all().context("flush sentinel temp file")
}

/// Write the patch-state document to its temp sibling and flush it, mirroring
/// [`write_quarantine_temp`] for the `patch-state.toml` document shape.
fn write_state_temp(root: &Path, temp: &Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(root).context("create publication folder")?;
    let mut file = std::fs::File::create(temp).context("create patch state temp file")?;
    file.write_all(text.as_bytes())
        .context("write patch state temp file")?;
    file.sync_all().context("flush patch state temp file")
}

/// File identity of the artifact named by a manifest that latched a terminal failure.
///
/// The manifest text alone cannot distinguish "the same bytes are still bad" from "the file was
/// repaired in place without rewriting the manifest". Recording the artifact's length and
/// modification time lets an identical-text `poll`/`stage_movement_processor` clear its latch and
/// retry once when the file changed. This is metadata only: the digest is not recomputed on every
/// poll, so a same-length repair whose modification time is unavailable or unchanged can still be
/// missed, and a `None` modified time compares by length only.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactStamp {
    library: String,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

/// Snapshot the artifact's metadata. `None` means the library is not a plain basename inside the
/// publication root, or the file cannot be stat'd (gone or unreadable). Callers treat a missing
/// current stamp as "retry" rather than "unchanged".
fn artifact_stamp(root: &Path, library: &str) -> Option<ArtifactStamp> {
    // A parsed manifest can name a path; refuse to stat outside the publication root, exactly as
    // `verify_publication` refuses the same name. Such a failure is repairable only by the text.
    if !plain_library_basename(library) {
        return None;
    }
    let metadata = std::fs::metadata(root.join(Path::new(library))).ok()?;
    Some(ArtifactStamp {
        library: library.to_owned(),
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

impl ArtifactStamp {
    /// Whether the named artifact still has the recorded length and, when the recorded modification
    /// time is present, the same modification time. A missing or unreadable artifact returns
    /// `false` so the caller retries instead of masking a repair.
    fn matches_current(&self, root: &Path) -> bool {
        artifact_stamp(root, &self.library).is_some_and(|current| {
            current.len == self.len
                && self
                    .modified
                    .map_or(true, |modified| current.modified == Some(modified))
        })
    }
}

struct Loaded {
    revision: String,
    digest: String,
    process: ProcessPose,
    filename: String,
    // This is deliberately owned beside the function pointer and never exposed to callers.
    // `None` exists only for the in-crate probes used by focused tests, which cannot build a
    // native library; production `Loaded::open` always pins `Some(library)`.
    _library: Option<Library>,
}

/// Typed pose evaluation failure raised by [`Loaded::evaluate`].
///
/// `Display` is exactly the existing free-form detail, so every caller that formatted the old
/// `anyhow` error as `{error}` or `{error:#}` produces the same log text. [`Self::reason`] exposes
/// the fine [`PatchEvaluationReason`] without parsing that text. Deliberately private: only the
/// host turns it into a typed slot or a restore outcome.
struct EvaluationFailure {
    reason: PatchEvaluationReason,
    detail: String,
}

impl EvaluationFailure {
    fn new(reason: PatchEvaluationReason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }

    fn reason(&self) -> PatchEvaluationReason {
        self.reason
    }
}

impl std::fmt::Display for EvaluationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::fmt::Debug for EvaluationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EvaluationFailure")
            .field("reason", &self.reason)
            .field("detail", &self.detail)
            .finish()
    }
}

fn digest_bytes(digest: &str) -> Option<ModuleDigest> {
    if digest.len() != 40 {
        return None;
    }
    let mut bytes = [0u8; 20];
    for (output, pair) in bytes.iter_mut().zip(digest.as_bytes().chunks_exact(2)) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        *output = ((high << 4) | low) as u8;
    }
    Some(bytes)
}

/// A publication verification or load failure that carries a stable [`PatchRejectReason`].
///
/// The reason is the fine, recorder-safe code; the printable `detail` stays separate so the
/// existing free-form log text is unchanged. `Display` is the previous outer context label (or the
/// plain verification/contract message); an error produced by `Library::new`/`library.get` is kept
/// as `inner` and exposed through [`std::error::Error::source`]. `anyhow`'s alternate formatter
/// appends the source chain, so the previous `{error:#}` text is reproduced exactly while
/// [`transient_io`] can still find a Windows sharing/lock `std::io::Error` underneath a failed
/// load.
struct PatchRejection {
    reason: PatchRejectReason,
    detail: &'static str,
    /// The wrapped source error for a failed `Library::new`/`library.get`, or `None` for a plain
    /// verification/contract message. Stored as an `anyhow::Error` so its whole source chain
    /// (including `libloading`'s `std::io::Error` source on Windows) survives.
    inner: Option<anyhow::Error>,
}

impl PatchRejection {
    /// A deterministic verification/contract failure with no wrapped source error.
    fn plain(reason: PatchRejectReason, detail: &'static str) -> Self {
        Self {
            reason,
            detail,
            inner: None,
        }
    }

    /// A load failure that keeps the context label as `Display` and exposes the native/source error
    /// through [`std::error::Error::source`].
    fn wrapped(reason: PatchRejectReason, detail: &'static str, inner: anyhow::Error) -> Self {
        Self {
            reason,
            detail,
            inner: Some(inner),
        }
    }

    fn into_error(self) -> anyhow::Error {
        anyhow::Error::new(self)
    }
}

impl std::fmt::Display for PatchRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Keep the previous outer context label; `anyhow`'s `{:#}` formatter appends the
        // `source()` chain, so embedding `inner` here would duplicate the wrapped load error.
        formatter.write_str(self.detail)
    }
}

impl std::fmt::Debug for PatchRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PatchRejection")
            .field("reason", &self.reason)
            .field("detail", &self.detail)
            .finish()
    }
}

impl std::error::Error for PatchRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // `anyhow::Error` implements both `AsRef<dyn Error>` and
        // `AsRef<dyn Error + Send + Sync>`; annotate the `Send + Sync` form so the call is
        // unambiguous, then upcast it to the plain error trait object.
        let inner: &(dyn std::error::Error + Send + Sync + 'static) = self.inner.as_ref()?.as_ref();
        Some(inner as &(dyn std::error::Error + 'static))
    }
}

/// Extract the fine reason from a [`verify_publication`]/[`Loaded::open`] error.
///
/// `None` means the failure is untyped/legacy, so the caller keeps its coarse fallback.
fn publication_reason(error: &anyhow::Error) -> Option<PatchRejectReason> {
    error
        .downcast_ref::<PatchRejection>()
        .map(|rejection| rejection.reason)
}

/// Verify a publication's artifact for one seam and return its pinned path and content digest.
///
/// The order is load-bearing and shared by every seam: basename, non-empty revision, size cap,
/// content digest, then the immutable `<prefix><digest>.dll` name. Every deterministic rejection
/// carries its fine [`PatchRejectReason`] in a [`PatchRejection`]; filesystem I/O (`metadata`,
/// `read`) stays a raw `std::io::Error` so the poll call site can keep classifying it as
/// transient. A digest mismatch is reported before the filename test so the diagnostic names the
/// bytes; `Host::poll` treats that deterministic mismatch as terminal, while an
/// unreadable/incomplete artifact keeps its retry.
fn verify_publication(
    root: &Path,
    publication: &Publication,
    kind: ModuleKind,
) -> Result<(PathBuf, String)> {
    let path = Path::new(&publication.library);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(PatchRejection::plain(
            PatchRejectReason::InvalidBasename,
            "library must be a basename inside the publication folder",
        )
        .into_error());
    }
    if publication.revision.is_empty() {
        return Err(
            PatchRejection::plain(PatchRejectReason::EmptyRevision, "empty revision").into_error(),
        );
    }
    let path = root.join(path);
    if std::fs::metadata(&path)?.len() > MAX_LIBRARY_BYTES {
        return Err(PatchRejection::plain(
            PatchRejectReason::ArtifactTooLarge,
            "module exceeds 64 MiB limit",
        )
        .into_error());
    }
    let bytes = std::fs::read(&path)?;
    let digest = format!("{:x}", Sha1::digest(&bytes));
    if digest != publication.sha1 {
        return Err(PatchRejection::plain(
            PatchRejectReason::DigestMismatch,
            "module digest differs from publication",
        )
        .into_error());
    }
    if publication.library != format!("{}{digest}.dll", kind.prefix()) {
        return Err(PatchRejection::plain(
            PatchRejectReason::ImmutableFilename,
            "module must use its immutable digest filename",
        )
        .into_error());
    }
    Ok((path, digest))
}

/// Whether `name` is one of this host's immutable `<prefix><40-hex>.dll` artifacts for `prefix`.
fn is_managed_artifact(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(".dll"))
        .is_some_and(|digest| digest.len() == 40 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Whether a loader error is a transient filesystem failure that the next 1 Hz poll may retry.
///
/// This is deliberately an allowlist over the cause chain: a not-yet-visible copy (`NotFound`), a
/// publisher sharing/lock violation (Windows raw OS error 32/33, which Rust reports as
/// `Uncategorized`, or `PermissionDenied`), or an interrupted/blocked/timeout read can succeed
/// unchanged. A plain string, TOML parse failure, digest mismatch or ABI error carries no transient
/// `std::io::Error` in its chain and is therefore a terminal content/contract failure.
fn transient_io(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
            ) || (cfg!(windows) && matches!(io.raw_os_error(), Some(32) | Some(33)))
        })
    })
}

/// Whether a publisher's exclusive `publish.lock` is present. A digest mismatch while the lock
/// is held may be an incomplete copy the publisher is still finishing; it is retried, not
/// latched. The supported publisher writes the artifact before swapping the manifest, so this
/// guard covers manual/non-atomic copies rather than the normal path.
fn publish_in_progress(root: &Path) -> bool {
    root.join("publish.lock").exists()
}

/// Whether the caller's movement frame and bounds carry usable scalar inputs. `dt_seconds` is
/// checked separately for a positive step; `liquid_height` uses `f32::NEG_INFINITY` as the "no
/// liquid" sentinel, so every other float must be finite. The caller's bounds must be non-negative
/// (`+INFINITY` remains the unbounded sentinel), and its frame may not seed a facing or speed
/// outside those bounds. Input `move_flags` may carry baseline/server-owned state; only the
/// module's *output* flags are checked against `permitted_flags`. A frame failing this check is a
/// caller error and never quarantines a processor.
fn movement_frame_valid(frame: &MovementFrame, bounds: &MovementBounds) -> bool {
    frame.position.iter().all(|value| value.is_finite())
        && frame.velocity.iter().all(|value| value.is_finite())
        && [
            frame.facing,
            frame.pitch,
            frame.camera_yaw,
            frame.run_speed,
            frame.walk_speed,
            frame.swim_speed,
            frame.ground_height,
            frame.stamina_norm,
        ]
        .iter()
        .all(|value| value.is_finite())
        && frame.ground_normal.iter().all(|value| value.is_finite())
        && (frame.liquid_height.is_finite() || frame.liquid_height == f32::NEG_INFINITY)
        && [
            bounds.max_step,
            bounds.max_horizontal_speed,
            bounds.max_vertical_speed,
            bounds.max_abs_facing,
        ]
        .iter()
        .all(|value| (*value >= 0.0 && value.is_finite()) || *value == f32::INFINITY)
        && frame.facing.abs() <= bounds.max_abs_facing
        && (frame.velocity[0] * frame.velocity[0] + frame.velocity[2] * frame.velocity[2]).sqrt()
            <= bounds.max_horizontal_speed
        && frame.velocity[1].abs() <= bounds.max_vertical_speed
}

impl Loaded {
    fn digest_bytes(&self) -> Option<ModuleDigest> {
        digest_bytes(&self.digest)
    }

    /// Load an artifact whose content identity and immutable filename were already checked by
    /// [`verify_publication`]. Keeping verification separate lets `Host::poll` quarantine only a
    /// digest it actually proved against the bytes, never a declared digest from a failed check.
    fn open(path: PathBuf, digest: String, publication: Publication) -> Result<Self> {
        // SAFETY: executable local modules are explicitly trusted. The caller publishes immutable
        // digest-named libraries. This owner pins code until every invocation has returned.
        let library = unsafe { Library::new(&path) }
            .map_err(|error| {
                PatchRejection::wrapped(
                    PatchRejectReason::ModuleLoadFailed,
                    "loading presentation module",
                    error.into(),
                )
                .into_error()
            })?;
        let entry = unsafe { library.get::<EntryPoint>(ENTRY) }
            .map_err(|error| {
                PatchRejection::wrapped(
                    PatchRejectReason::MissingEntryPoint,
                    "missing module entry point",
                    error.into(),
                )
                .into_error()
            })?;
        let api = unsafe { entry() };
        if api.is_null() {
            return Err(PatchRejection::plain(
                PatchRejectReason::AbiMismatch,
                "null module descriptor",
            )
            .into_error());
        }
        let header = unsafe { std::ptr::read(api.cast::<Header>()) };
        if !header.compatible() {
            return Err(PatchRejection::plain(
                PatchRejectReason::AbiMismatch,
                "incompatible presentation ABI",
            )
            .into_error());
        }
        let Some(process) = (unsafe { (*api).process }) else {
            return Err(PatchRejection::plain(
                PatchRejectReason::MissingProcessor,
                "missing pose processor",
            )
            .into_error());
        };
        Ok(Self {
            revision: publication.revision,
            digest,
            process,
            filename: publication.library,
            _library: Some(library),
        })
    }

    /// A processor handle for the focused tests that cannot build a native library.
    ///
    /// Production code only reaches [`Self::open`], which pins `Some(library)`. The probe is
    /// deliberately test-only so no non-pinned processor can be staged by a caller.
    #[cfg(test)]
    fn probe(revision: &str, digest: &str, process: ProcessPose) -> Self {
        Self {
            revision: revision.to_owned(),
            digest: digest.to_owned(),
            process,
            filename: format!("{}{digest}.dll", ModuleKind::Pose.prefix()),
            _library: None,
        }
    }

    fn evaluate(
        &self,
        bones: &[BoneLocal],
        rigs: &[RigSpan],
        seconds: f64,
        output: &mut Vec<BoneLocal>,
    ) -> std::result::Result<(), EvaluationFailure> {
        if bones.len() > u32::MAX as usize || rigs.len() > u32::MAX as usize {
            return Err(EvaluationFailure::new(
                PatchEvaluationReason::FrameTooLarge,
                "pose frame too large",
            ));
        }
        output.clear();
        output.extend_from_slice(bones);
        let frame = PoseFrame {
            bones: bones.as_ptr(),
            bone_count: bones.len() as u32,
            rigs: rigs.as_ptr(),
            rig_count: rigs.len() as u32,
            time_seconds: seconds,
        };
        let status = unsafe { (self.process)(&frame, output.as_mut_ptr(), output.len() as u32) };
        if status != 0 {
            return Err(EvaluationFailure::new(
                PatchEvaluationReason::Status(status),
                format!(
                    "module {} rejected pose frame (status {status})",
                    self.revision
                ),
            ));
        }
        // Check every transform before the topology check so an output that is both invalid and
        // topology-changing is classified `InvalidOutput`; `TopologyChanged` therefore wins only
        // when the transforms themselves are valid.
        if output.iter().any(|after| !after.valid()) {
            return Err(EvaluationFailure::new(
                PatchEvaluationReason::InvalidOutput,
                "module returned invalid transforms or changed skeleton topology",
            ));
        }
        if output
            .iter()
            .zip(bones)
            .any(|(after, before)| after.parent != before.parent)
        {
            return Err(EvaluationFailure::new(
                PatchEvaluationReason::TopologyChanged,
                "module returned invalid transforms or changed skeleton topology",
            ));
        }
        Ok(())
    }
}

// ============================================================================
// Seam 2: movement processor staging (`MovementModuleV1`)
//
// `Host::stage_movement_processor` consults the host-owned [`ModularGameplayRegistry`] before it
// opens any artifact: a disabled movement seam is a typed refusal, not a publication error, and
// no DLL artifact bytes are read or mapped. (`movement.toml` itself is read first only to tell an
// absent publication from a disabled seam.) The registry is therefore a live capability gate
// rather than a bookkeeping-only record.
// ============================================================================

/// Host-owned bounds for one movement proposal. The caller derives them from the baseline
/// controller's own frame, so the host never accepts a proposal the baseline could not have made.
/// Speeds are yards/second in the caller's axes; `max_step` is this frame's travel budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementBounds {
    pub max_step: f32,
    pub max_horizontal_speed: f32,
    pub max_vertical_speed: f32,
    pub max_abs_facing: f32,
    /// The only move-flag bits the seam delegates. A proposal setting anything else is refused
    /// rather than masked: root, falling, swimming, transport and the granted modes stay with the
    /// server and the baseline controller.
    pub permitted_flags: u32,
}

/// A fully validated movement proposal in host-owned scalar form. No pointers, handles, Rust or
/// Bevy types cross this boundary, and nothing here can write the game socket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementProposal {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub facing: f32,
    pub pitch: f32,
    pub move_flags: u32,
}

/// Which slot produced a validated proposal. A `Staged` candidate is not active yet: the
/// controller must accept the same proposal and only then call [`Host::commit_movement_processor`],
/// which refuses while no proposal is armed. A controller rejection must call
/// [`Host::withdraw_movement_proposal`] before the next commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementProposalSource {
    Staged,
    Active,
}

/// Typed refusal from the movement seam. A [`Host::movement_proposal`] `Err` leaves no proposal
/// written, but it does not always drop the movement slot: `FrameInput` (an unusable caller frame)
/// and `NoModule` leave the staged/active selection intact, so the caller hands prediction back for
/// this frame and can use the same processor on the next valid frame. A module-produced `Status` or
/// output-validation refusal is counted against the active processor's [`MovementHealth`] and
/// [`MovementHealthPolicy`]: below both enabled thresholds it is tolerated
/// ([`MovementRefusal::Tolerated`]) and the active processor is retained; at either threshold it
/// drops the failing candidate and quarantines its verified digest. A failing staged candidate is
/// always single-strike (its canary failure never changes active health) and retains a healthy
/// active processor with a different digest, while a failing active processor clears both slots.
/// [`MovementRefusal::SeamDisabled`] evicts both. A
/// [`Host::stage_movement_processor`] `Err` drops only the staged candidate and latches as its
/// variant describes; it does not evict an already-active processor except for
/// [`MovementRefusal::SeamDisabled`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MovementRefusal {
    /// No movement module is staged or active.
    NoModule,
    /// An active processor failed below both enabled [`MovementHealthPolicy`] thresholds. The
    /// processor stays active and is not quarantined; the caller renders the baseline movement for
    /// this frame and may evaluate the same processor on the next valid frame. The precise
    /// module-produced refusal remains in [`MovementHealthSnapshot::last_refusal`] and the
    /// free-form detail in [`Host::take_movement_rejection`].
    Tolerated,
    /// The movement seam was disabled in the host's [`ModularGameplayRegistry`]. The host refused
    /// before reading or loading the DLL artifact, dropped any staged/active movement slot and
    /// latched the manifest text so garbage collection keeps its artifact. Enabling the seam
    /// clears that latch so the same manifest is staged again.
    SeamDisabled,
    /// Reading, parsing or verifying `movement.toml` failed. Free-form detail is kept in
    /// [`Host::take_movement_rejection`] for logs.
    Publication,
    /// The manifest named a digest the host quarantined after a terminal post-verification
    /// `MovementLoaded::open` failure or a module-produced `Status`/output-validation failure. It
    /// is refused before any artifact read or `Library::new`, even when a changed manifest
    /// (`B -> C -> B`) names the same bad artifact again.
    Quarantined,
    /// The descriptor magic, artifact prefix or ABI version named another seam or version.
    Kind,
    /// The descriptor's frame/output layout differs from the running host ABI.
    Layout,
    /// The descriptor published no callable processor (`null` descriptor or process pointer).
    MissingProcessor,
    /// The frame or bounds handed in were not usable: a non-finite/non-positive clock step, a
    /// non-finite scalar field (other than a documented sentinel), invalid bounds, or a
    /// caller-seeded facing/speed outside them.
    FrameInput,
    /// The module returned a non-zero status.
    Status(i32),
    /// A proposal value was not finite.
    NonFinite,
    /// The proposal moved further than one frame's travel budget.
    Step(f32),
    /// The proposed horizontal or vertical speed exceeded the frame cap.
    Speed(f32),
    /// The proposed facing left the permitted range.
    Facing(f32),
    /// The proposal set a move-flag bit the seam does not delegate.
    Flags(u32),
    /// The proposal asked the host to emit a movement opcode. `wrath_stream_movement` remains the
    /// sole wire writer, so this is refused rather than forwarded.
    WireWrite,
}

/// Read-only health snapshot of the currently active movement processor.
///
/// The counters and timings describe the active processor's steady-state
/// [`Host::movement_proposal`] calls; a staged candidate's canary call is an activation probe and
/// is neither timed nor counted. `revision`/`digest` name the processor active at snapshot time, so
/// both are `None` after a demotion or unload while the last counters remain visible after a
/// demotion. `last_refusal` is the most recent module-produced failure (`Status`/validation), even
/// while the failure was tolerated; it is cleared by a successful active proposal, by a commit and
/// by an unload/seam disable. A tolerated failure leaves `movement_active` in place, so the caller
/// sees the same identity beside the incremented counters. The persisted per-revision store is
/// written separately by [`Host::persist_state`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct MovementHealthSnapshot {
    /// Revision of the processor active at snapshot time, or `None` when the slot is empty.
    pub revision: Option<String>,
    /// Verified SHA-1 of the processor active at snapshot time, or `None` when the slot is empty.
    pub digest: Option<String>,
    /// Active-processor failures since its last valid proposal or activation.
    pub consecutive_frame_failures: u32,
    /// Failures recorded for this verified digest since it was first seen, cumulative across
    /// reactivations and process runs. [`MovementHealthPolicy::max_total_frame_failures`] compares
    /// it; a commit seeds it from `patch-state.toml`, while the consecutive count starts at 0.
    pub total_frame_failures: u32,
    /// Duration of the most recent steady-state active evaluation, in microseconds. A staged
    /// candidate's canary call is not timed, so this is 0 until the first active proposal.
    pub last_eval_micros: u64,
    /// High-water mark of steady-state active evaluation duration, in microseconds. Seeded from
    /// the per-revision store on commit.
    pub max_eval_micros: u64,
    /// Number of movement digests currently held in the bounded quarantine.
    pub quarantined_digests: usize,
    /// Most recent module-produced failure, or `None` after activation/success. A tolerated
    /// failure keeps it while the active processor is retained.
    pub last_refusal: Option<MovementRefusal>,
}

/// Demotion thresholds for the active movement processor.
///
/// Both thresholds count the active processor's failed proposals, not staged-candidate canary
/// failures. The consecutive threshold restarts on every commit (staged becomes active); the total
/// threshold is per-revision cumulative across reactivations and process runs, seeded from
/// `patch-state.toml`. The thresholds are independent alternatives: a failure demotes as soon as
/// *either* enabled threshold is reached, and a value of `0` disables that threshold. `0/0`
/// therefore never demotes on health counters alone. The default is the original single-strike
/// behavior (`1/3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovementHealthPolicy {
    /// Consecutive active-processor failures that demote it. `0` disables this threshold.
    pub max_consecutive_frame_failures: u32,
    /// Per-revision cumulative active-processor failures that demote it. `0` disables this
    /// threshold. The count is seeded across reactivations from `patch-state.toml`.
    pub max_total_frame_failures: u32,
}

impl Default for MovementHealthPolicy {
    fn default() -> Self {
        Self {
            max_consecutive_frame_failures: 1,
            max_total_frame_failures: 3,
        }
    }
}

/// Why a loaded movement artifact could not be staged.
struct MovementLoadFailure {
    refusal: MovementRefusal,
    detail: String,
    /// Whether the same manifest may succeed unchanged: a transient filesystem availability error
    /// anywhere in the cause chain, or a publisher-held `publish.lock`. A transient failure is
    /// retried without latching the manifest or quarantining a digest.
    transient: bool,
    /// `Some` only after [`verify_publication`] proved the bytes; `None` for every verification
    /// failure. Only a proved digest may be quarantined, so a declared-but-unproved digest never
    /// poisons a legitimate same-digest repair.
    verified_digest: Option<String>,
}

/// A loaded `MovementModuleV1` candidate or active processor.
struct MovementLoaded {
    revision: String,
    digest: String,
    filename: String,
    process: ProcessMovement,
    /// Pins the mapped code while any slot references it. `None` only for the in-crate probe used
    /// by focused tests, which cannot build a native library; production staging always pins.
    _library: Option<Library>,
}

impl MovementLoaded {
    /// Verify a movement publication's artifact and return its pinned path and content digest.
    ///
    /// This contains exactly the former `verify_publication` block of [`Self::open`], with the same
    /// error mapping: `refusal: Publication`, `transient: transient_io(&error) || publish_in_progress(root)`,
    /// `verified_digest: None`. It performs no native load.
    fn verify(
        root: &Path,
        publication: &Publication,
    ) -> std::result::Result<(PathBuf, String), MovementLoadFailure> {
        verify_publication(root, publication, ModuleKind::Movement).map_err(|error| {
            MovementLoadFailure {
                refusal: MovementRefusal::Publication,
                detail: format!("movement publication rejected: {error:#}"),
                transient: transient_io(&error) || publish_in_progress(root),
                verified_digest: None,
            }
        })
    }

    fn open(
        root: &Path,
        path: PathBuf,
        digest: String,
        publication: Publication,
    ) -> std::result::Result<Self, MovementLoadFailure> {
        // Verification (including the publish.lock check) already ran in [`Self::verify`]; the
        // original publication root is still passed so the transient `publish_in_progress` check
        // below is byte-for-byte the pre-split behavior.
        // SAFETY: executable local modules are explicitly trusted, exactly as for the pose seam.
        let library = unsafe { Library::new(&path) }
            .context("loading presentation module")
            .map_err(|error| {
                // `libloading` exposes the underlying `std::io::Error` as `Error::source()` on
                // Windows, so the anyhow chain still sees a publisher's sharing/lock failure (raw
                // 32/33) while a bad image or missing dependency stays terminal. On Unix `DlOpen`
                // carries no io source, so load-time errors are terminal and only
                // `verify_publication`'s reads can be transient.
                MovementLoadFailure {
                    refusal: MovementRefusal::Publication,
                    detail: format!("movement publication rejected: {error:#}"),
                    transient: transient_io(&error) || publish_in_progress(root),
                    verified_digest: Some(digest.clone()),
                }
            })?;
        let entry =
            unsafe { library.get::<benilla_hotpatch_abi::MovementEntryPoint>(MOVEMENT_ENTRY) }
                .map_err(|error| {
                    let error = anyhow::Error::new(error);
                    MovementLoadFailure {
                        refusal: MovementRefusal::Publication,
                        detail: format!("movement publication rejected: {error:#}"),
                        transient: transient_io(&error) || publish_in_progress(root),
                        verified_digest: Some(digest.clone()),
                    }
                })?;
        let descriptor = unsafe { entry() };
        if descriptor.is_null() {
            return Err(MovementLoadFailure {
                refusal: MovementRefusal::MissingProcessor,
                detail: format!(
                    "movement module {} returned a null descriptor",
                    publication.revision
                ),
                transient: publish_in_progress(root),
                verified_digest: Some(digest.clone()),
            });
        }
        let descriptor = unsafe { std::ptr::read(descriptor) };
        let process =
            validate_movement_descriptor(&descriptor).map_err(|refusal| MovementLoadFailure {
                refusal,
                detail: format!(
                    "movement module {} descriptor refused ({refusal:?})",
                    publication.revision
                ),
                transient: publish_in_progress(root),
                verified_digest: Some(digest.clone()),
            })?;
        Ok(Self {
            revision: publication.revision,
            digest,
            filename: publication.library,
            process,
            _library: Some(library),
        })
    }

    /// A processor handle for the focused tests that cannot build a native library, with a
    /// caller-chosen content identity so a staged candidate and an active processor can be
    /// distinguished by digest.
    #[cfg(test)]
    fn probe_with_digest(revision: &str, digest: &str, process: ProcessMovement) -> Self {
        Self {
            revision: revision.to_owned(),
            filename: format!("{}{digest}.dll", ModuleKind::Movement.prefix()),
            digest: digest.to_owned(),
            process,
            _library: None,
        }
    }

    /// A probe with a fixed all-zero digest for the tests that do not need distinct identities.
    #[cfg(test)]
    fn probe(revision: &str, process: ProcessMovement) -> Self {
        Self::probe_with_digest(revision, &"0".repeat(40), process)
    }
}

/// The movement slot selected for one proposal. `digest` is the selected module's verified content
/// identity, so a module-produced failure can quarantine exactly that artifact before the slot is
/// dropped.
struct MovementSelection {
    process: ProcessMovement,
    revision: String,
    digest: String,
    staged: bool,
}

/// Validate a movement module descriptor before any call into the candidate.
pub fn validate_movement_descriptor(
    descriptor: &MovementModuleV1,
) -> std::result::Result<ProcessMovement, MovementRefusal> {
    if descriptor.magic != MOVEMENT_MAGIC || descriptor.abi_version != ABI_VERSION {
        return Err(MovementRefusal::Kind);
    }
    if descriptor.frame_size != std::mem::size_of::<MovementFrame>() as u32
        || descriptor.output_size != std::mem::size_of::<MovementOutput>() as u32
    {
        return Err(MovementRefusal::Layout);
    }
    descriptor.process.ok_or(MovementRefusal::MissingProcessor)
}

/// Validate one proposal against the host's bounds. Pure: no module or host state is touched, so a
/// caller can refuse without committing anything.
pub fn validate_movement_output(
    input: &MovementFrame,
    output: &MovementOutput,
    bounds: &MovementBounds,
) -> std::result::Result<(), MovementRefusal> {
    if !output.valid() {
        return Err(MovementRefusal::NonFinite);
    }
    // A module never writes the movement stream: `wrath_stream_movement` is the sole writer.
    if output.emit_move_opcode != 0 {
        return Err(MovementRefusal::WireWrite);
    }
    if output.move_flags & !bounds.permitted_flags != 0 {
        return Err(MovementRefusal::Flags(
            output.move_flags & !bounds.permitted_flags,
        ));
    }
    if bounds.max_abs_facing.is_finite() && output.facing.abs() > bounds.max_abs_facing {
        return Err(MovementRefusal::Facing(output.facing));
    }
    let dx = output.position[0] - input.position[0];
    let dy = output.position[1] - input.position[1];
    let dz = output.position[2] - input.position[2];
    let step = (dx * dx + dy * dy + dz * dz).sqrt();
    if !step.is_finite() || step > bounds.max_step {
        return Err(MovementRefusal::Step(step));
    }
    let horizontal =
        (output.velocity[0] * output.velocity[0] + output.velocity[2] * output.velocity[2]).sqrt();
    if !horizontal.is_finite() || horizontal > bounds.max_horizontal_speed {
        return Err(MovementRefusal::Speed(horizontal));
    }
    let vertical = output.velocity[1].abs();
    if !vertical.is_finite() || vertical > bounds.max_vertical_speed {
        return Err(MovementRefusal::Speed(vertical));
    }
    Ok(())
}

fn movement_proposal_of(output: &MovementOutput) -> MovementProposal {
    MovementProposal {
        position: output.position,
        velocity: output.velocity,
        facing: output.facing,
        pitch: output.pitch,
        move_flags: output.move_flags,
    }
}

/// Live counters/timings for the active movement processor.
///
/// Mirrors [`PatchHealth`] for the movement seam: reset on every commit so the consecutive streak
/// and last sample describe the processor currently active, while the cumulative total and
/// evaluation high-water mark are seeded from the bounded per-revision store so they survive a
/// reactivation and a process restart. A staged candidate's canary call is an activation probe and
/// never touches this record.
#[derive(Clone, Debug, Default)]
struct MovementHealth {
    consecutive_frame_failures: u32,
    total_frame_failures: u32,
    last_eval_micros: u64,
    max_eval_micros: u64,
    last_refusal: Option<MovementRefusal>,
}

impl MovementHealth {
    /// Record one active-processor evaluation. The `u128` microsecond count saturates to `u64`.
    fn record_eval(&mut self, elapsed: std::time::Duration) {
        let micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
        self.last_eval_micros = micros;
        self.max_eval_micros = self.max_eval_micros.max(micros);
    }

    /// An active processor produced a valid proposal: the consecutive streak and the last refusal
    /// end, while the per-revision cumulative total survives until the next activation.
    fn record_success(&mut self) {
        self.consecutive_frame_failures = 0;
        self.last_refusal = None;
    }

    /// A module-produced active failure. The precise refusal is kept for the snapshot/detail API.
    fn record_failure(&mut self, refusal: MovementRefusal) {
        self.consecutive_frame_failures = self.consecutive_frame_failures.saturating_add(1);
        self.total_frame_failures = self.total_frame_failures.saturating_add(1);
        self.last_refusal = Some(refusal);
    }

    /// Whether the recorded failures reached either enabled threshold in `policy`.
    ///
    /// The thresholds are alternatives; a `0` value disables that threshold, so `0/0` never
    /// demotes on health counters.
    fn threshold_reached(&self, policy: MovementHealthPolicy) -> bool {
        (policy.max_consecutive_frame_failures != 0
            && self.consecutive_frame_failures >= policy.max_consecutive_frame_failures)
            || (policy.max_total_frame_failures != 0
                && self.total_frame_failures >= policy.max_total_frame_failures)
    }

    /// A newly committed processor restarts the consecutive streak, last sample and last refusal;
    /// the total and high-water mark are then seeded from `patch-state.toml`.
    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Live counters/timings for the active pose module.
///
/// Reset on every activation so the consecutive streak and last sample always describe the module
/// that is currently (or was most recently) active. The cumulative total and evaluation high-water
/// mark are then seeded from the bounded per-revision store, so they survive a reactivation and a
/// process restart. [`PatchHealthPolicy`] decides whether a recorded failure demotes; remaining
/// movement/rollback typed reasons remain out of scope (movement health has its own
/// [`MovementHealth`] record).
#[derive(Clone, Debug, Default)]
struct PatchHealth {
    consecutive_frame_failures: u32,
    total_frame_failures: u32,
    last_eval_micros: u64,
    max_eval_micros: u64,
}

impl PatchHealth {
    /// Record one active-module evaluation. The `u128` microsecond count saturates to `u64` rather
    /// than panicking on an implausibly long call.
    fn record_eval(&mut self, elapsed: std::time::Duration) {
        let micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
        self.last_eval_micros = micros;
        self.max_eval_micros = self.max_eval_micros.max(micros);
    }

    /// An active module produced a valid batch: the consecutive streak ends. The per-revision
    /// cumulative total survives; it is seeded from `patch-state.toml` on a later activation.
    fn record_success(&mut self) {
        self.consecutive_frame_failures = 0;
    }

    fn record_failure(&mut self) {
        self.consecutive_frame_failures = self.consecutive_frame_failures.saturating_add(1);
        self.total_frame_failures = self.total_frame_failures.saturating_add(1);
    }

    /// Whether the recorded failures reached either enabled threshold in `policy`.
    ///
    /// The two thresholds are alternatives, not a conjunction: a consecutive streak can demote
    /// before the cumulative cap, and failures spread across recoveries can demote without a long
    /// streak. A `0` value disables that threshold, so `0/0` never demotes on health counters.
    fn threshold_reached(&self, policy: PatchHealthPolicy) -> bool {
        (policy.max_consecutive_frame_failures != 0
            && self.consecutive_frame_failures >= policy.max_consecutive_frame_failures)
            || (policy.max_total_frame_failures != 0
                && self.total_frame_failures >= policy.max_total_frame_failures)
    }

    /// A newly activated module restarts the consecutive streak and last sample. The cumulative
    /// total and high-water mark are then seeded from `patch-state.toml` through
    /// [`Host::seed_persisted_health`]; its first steady-state evaluation on a later frame
    /// populates the timings (the candidate canary/restore probe is not timed).
    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Typed reason for a pose publication poll rejection.
///
/// The free-form loader/verification detail remains in the returned error and local log; this is
/// the stable code carried by `HotpatchEvent::PublicationRejected`. Verification and load failures
/// carry the fine code in the private `PatchRejection` wrapper; the coarse
/// [`Self::ArtifactRejected`] and [`Self::LoadFailed`] values are fallbacks for an untyped/legacy
/// error. Canary, active-frame, movement and rollback reasons remain H03.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatchRejectReason {
    /// A transient filesystem/publisher condition (`NotFound`, sharing/lock, interrupted read).
    TransientIoPending,
    /// `ready.toml` could not be decoded (for example, non-UTF-8 content).
    ManifestUnreadable,
    /// `ready.toml` parsed as TOML but not as a publication.
    ManifestMalformed,
    /// `publication.library` is not a single normal component inside the publication folder.
    InvalidBasename,
    /// `publication.revision` is empty.
    EmptyRevision,
    /// The artifact exceeds `MAX_LIBRARY_BYTES`.
    ArtifactTooLarge,
    /// The computed SHA-1 differs from `publication.sha1`.
    DigestMismatch,
    /// The filename is not exactly `<kind-prefix><digest>.dll`.
    ImmutableFilename,
    /// Fallback for an untyped/legacy pre-load artifact verification failure.
    ArtifactRejected,
    /// The declared digest is in the bounded quarantine store.
    Quarantined,
    /// `Library::new` failed for a terminal image/dependency reason. A sharing/lock failure is
    /// classified `TransientIoPending` by the poll gate instead.
    ModuleLoadFailed,
    /// The module exports no entry symbol.
    MissingEntryPoint,
    /// The entry returned a null descriptor or `Header::compatible()` failed.
    AbiMismatch,
    /// The descriptor has no callable `process` pointer.
    MissingProcessor,
    /// Fallback for an untyped/legacy post-verification load failure.
    LoadFailed,
    /// No typed reason was recorded (adapter fallback).
    Unknown,
}

pub struct Host {
    root: PathBuf,
    observed: Option<String>,
    /// Artifact identity recorded when a terminal pose failure latched [`Self::observed`]. An
    /// identical manifest text whose artifact length/mtime no longer matches clears the latch and
    /// retries once; malformed TOML and the digest quarantine record no stamp.
    pose_failure_stamp: Option<ArtifactStamp>,
    active: Option<Loaded>,
    pending: Option<Loaded>,
    previous: VecDeque<Loaded>,
    /// In-memory counters/timings for the active pose module, exposed through
    /// [`Host::health_snapshot`]. See [`PatchHealthSnapshot`].
    health: PatchHealth,
    /// Demotion thresholds compared against [`Self::health`] after an active-module failure.
    /// Defaults to the single-strike behavior; loaded from and saved to `patch-state.toml`.
    /// See [`PatchHealthPolicy`].
    health_policy: PatchHealthPolicy,
    /// Bounded per-revision cumulative health behind `patch-state.toml`, keyed by verified digest
    /// and kept oldest-first. Activation seeds [`Self::health`]'s total and high-water mark from
    /// the entry; steady-state evaluations update it. It grants nothing: a digest only seeds
    /// counters for a later verified activation, never selects code.
    persisted_health: VecDeque<PersistedRevisionHealth>,
    /// Whether the policy, persisted health or a seam artifact stamp changed since the last
    /// successful load/save. Only a changed persisted field sets it, so a per-frame success with an
    /// unchanged high-water mark does not rewrite the file.
    state_dirty: bool,
    rejection: Option<String>,
    /// Typed reason paired with [`Self::rejection`]. Consumed together through
    /// [`Host::take_rejection_detail`] or cleared by [`Host::take_rejection`].
    rejection_reason: Option<PatchEvaluationReason>,
    /// Typed reason for the most recent pose publication poll rejection. Cleared at the start of
    /// every poll and consumed through [`Host::take_publication_reason`].
    publication_reason: Option<PatchRejectReason>,
    active_failure: Option<String>,
    /// Typed reason paired with [`Self::active_failure`]. Consumed together through
    /// [`Host::take_active_failure_detail`] or cleared by [`Host::take_active_failure`].
    active_failure_reason: Option<PatchEvaluationReason>,
    restores: VecDeque<RestoreOutcome>,
    /// Bounded quarantine (in-memory plus `quarantine.toml`) of pose digests rejected after their bytes were verified: a
    /// terminal `Loaded::open` failure, a rejected canary batch, an active frame failure that
    /// reached a [`PatchHealthPolicy`] threshold, or a failing restore predecessor. The key is
    /// `Publication.sha1`, so a later manifest text that
    /// names the same bad artifact (`B -> C -> B`) is refused before `Library::new` without
    /// re-reading the DLL. A frame rejected by host validation never reaches this store. The list
    /// is persisted to `quarantine.toml` (see [`Host::persist_quarantine`]) and restored at startup
    /// (see [`Host::load_persistent_quarantine`]); remaining movement/rollback typed reasons remain
    /// H03 (movement health is host-current and persisted).
    /// The pose loading and staged-canary sentinels and the movement loading sentinel are
    /// implemented in this slice.
    quarantined_digests: VecDeque<String>,
    /// The pose/movement crash-sentinel error slot. Deliberately separate from
    /// `Host::observed`/`self.rejection` and from the quarantine slots: a best-effort sentinel write
    /// or clear failure must never fail or skip the module load/evaluation, only surface here.
    sentinel_error: Option<String>,
    /// Digest recovered from an unclean pose marker but not yet committed to
    /// `quarantine.toml`. While set, the marker is retained and a new pose or movement load/canary
    /// must commit the recovered quarantine before writing a replacement marker.
    sentinel_recovery_pending: Option<String>,
    /// The exact `active-sentinel.toml` identity this host last wrote or accepted: the
    /// [`ActiveSentinelFile`] contents. `Some` after [`Host::write_active_sentinel`] published a
    /// marker or [`Host::load_active_sentinel`] validated one; `None` otherwise. Ownership is bound
    /// to the marker *content* (revision, sha1, kind, stage), not to the file path: a later writer
    /// that replaces the file with different content is no longer owned and is retained by the
    /// clean-shutdown safety net, and a marker present before either event (an unvalidated
    /// previous-run marker, a malformed file, or a marker owned by another process) is never
    /// removed either.
    sentinel_marker_owned: Option<ActiveSentinelFile>,
    /// The movement seam's own manifest latch, independent of the pose publication.
    movement_observed: Option<String>,
    /// Artifact identity recorded when a terminal movement failure latched
    /// [`Self::movement_observed`]. Enabling the seam clears it with the latch; disabling keeps
    /// both so the retention sweep still pins the refused publication.
    movement_failure_stamp: Option<ArtifactStamp>,
    movement_pending: Option<MovementLoaded>,
    movement_active: Option<MovementLoaded>,
    movement_refusal: Option<MovementRefusal>,
    movement_rejection: Option<String>,
    /// Whether the most recent [`Self::movement_proposal`] call produced a validated proposal from
    /// the currently staged candidate. [`Self::commit_movement_processor`] promotes only when this
    /// is `true`, so a staged candidate cannot be activated without a host-validated proposal from
    /// that same candidate. Cleared at the start of every proposal call, on every refusal, and
    /// whenever the selected or staged state changes (a new stage, unload or seam toggle).
    movement_proposal_ready: bool,
    /// One-shot guard for the movement canary crash sentinel: the first staged movement proposal
    /// writes the marker immediately before `process`; every later staged proposal and every active
    /// proposal leaves it unset. Reset whenever a new staged candidate is installed, or a candidate
    /// is unloaded/committed/rejected, so a fresh staging can mark again.
    movement_canary_attempted: bool,
    /// Bounded quarantine (in-memory plus `quarantine.toml`) of movement digests rejected after their bytes were verified:
    /// a terminal post-verification `MovementLoaded::open` failure, or a module-produced `Status`
    /// or output-validation failure in `movement_proposal`. A changed manifest that still names
    /// the same bad artifact (`B -> C -> B`) is refused before any artifact read or `Library::new`.
    /// Deliberately separate from the pose store so the same bytes under a different ABI cannot
    /// suppress the other seam; H03 may unify the stores under a `(ModuleKind, digest)` key. Both
    /// lists travel in the same `quarantine.toml` but stay separate in memory. Movement/rollback
    /// typed reasons and the movement active-frame sentinel remain H03 (movement health counters
    /// are host-current and persisted).
    movement_quarantined_digests: VecDeque<String>,
    /// Live counters/timings for the active movement processor, exposed through
    /// [`Host::movement_health_snapshot`]. A staged candidate's canary call never touches it; a
    /// commit seeds its total/high-water mark from [`Self::persisted_movement_health`].
    movement_health: MovementHealth,
    /// Demotion thresholds compared against [`Self::movement_health`] after an active movement
    /// failure. Defaults to the single-strike `1/3`; changed through
    /// [`Host::set_movement_health_policy`].
    movement_health_policy: MovementHealthPolicy,
    /// Bounded per-revision cumulative movement health behind `patch-state.toml`, keyed by verified
    /// digest and kept oldest-first. A commit seeds [`Self::movement_health`]'s total and
    /// high-water mark from the entry; steady-state evaluations update it. It grants nothing: a
    /// digest only seeds counters for a later verified commit, never selects code.
    persisted_movement_health: VecDeque<PersistedRevisionHealth>,
    /// Which hot-pluggable gameplay seams are enabled. Pose is always enabled; the movement
    /// staging path consults this before it opens an artifact.
    seams: ModularGameplayRegistry,
}

impl Host {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            observed: None,
            pose_failure_stamp: None,
            active: None,
            pending: None,
            previous: VecDeque::new(),
            health: PatchHealth::default(),
            health_policy: PatchHealthPolicy::default(),
            persisted_health: VecDeque::new(),
            state_dirty: false,
            rejection: None,
            rejection_reason: None,
            publication_reason: None,
            active_failure: None,
            active_failure_reason: None,
            restores: VecDeque::new(),
            quarantined_digests: VecDeque::new(),
            sentinel_error: None,
            sentinel_recovery_pending: None,
            sentinel_marker_owned: None,
            movement_observed: None,
            movement_failure_stamp: None,
            movement_pending: None,
            movement_active: None,
            movement_refusal: None,
            movement_rejection: None,
            movement_proposal_ready: false,
            movement_canary_attempted: false,
            movement_quarantined_digests: VecDeque::new(),
            movement_health: MovementHealth::default(),
            movement_health_policy: MovementHealthPolicy::default(),
            persisted_movement_health: VecDeque::new(),
            seams: ModularGameplayRegistry::new(),
        }
    }

    /// Add a pose digest to the bounded quarantine (in-memory plus `quarantine.toml`). Deduplicates so a repeated failure
    /// does not grow the store, and trims the oldest entry at the cap. Callers add only a digest
    /// whose bytes were verified or that came from an already loaded module; a frame rejected by
    /// the host validation in `evaluate` does not reach this method.
    fn quarantine_digest(&mut self, digest: &str) {
        if self.quarantined_digests.iter().any(|d| d == digest) {
            return;
        }
        if self.quarantined_digests.len() == MAX_QUARANTINED_DIGESTS {
            self.quarantined_digests.pop_front();
        }
        self.quarantined_digests.push_back(digest.to_owned());
    }

    /// Whether a pose digest was quarantined after a terminal load or evaluation failure.
    fn is_quarantined(&self, digest: &str) -> bool {
        self.quarantined_digests.iter().any(|d| d == digest)
    }

    /// Add a movement digest to the bounded quarantine (in-memory plus `quarantine.toml`). Same semantics as the pose
    /// store: deduplicate a repeated failure and evict the oldest entry at the cap.
    fn quarantine_movement_digest(&mut self, digest: &str) {
        if self
            .movement_quarantined_digests
            .iter()
            .any(|d| d == digest)
        {
            return;
        }
        if self.movement_quarantined_digests.len() == MAX_QUARANTINED_DIGESTS {
            self.movement_quarantined_digests.pop_front();
        }
        self.movement_quarantined_digests
            .push_back(digest.to_owned());
    }

    /// Whether a movement digest was quarantined after a terminal post-verification
    /// `MovementLoaded::open` failure or a module-produced proposal failure.
    fn is_movement_quarantined(&self, digest: &str) -> bool {
        self.movement_quarantined_digests.iter().any(|d| d == digest)
    }

    /// Replace the pose artifact stamp and mark the state dirty when it actually changes.
    /// Setting or clearing a stamp changes the persisted `patch-state.toml` document even though
    /// it is best-effort recovery metadata, never a digest trust decision.
    fn set_pose_failure_stamp(&mut self, stamp: Option<ArtifactStamp>) {
        if self.pose_failure_stamp != stamp {
            self.pose_failure_stamp = stamp;
            self.state_dirty = true;
        }
    }

    /// Replace the movement artifact stamp and mark the state dirty when it actually changes.
    fn set_movement_failure_stamp(&mut self, stamp: Option<ArtifactStamp>) {
        if self.movement_failure_stamp != stamp {
            self.movement_failure_stamp = stamp;
            self.state_dirty = true;
        }
    }

    /// Reset the live health record for a newly active digest and seed its cumulative values from
    /// the bounded per-revision store.
    ///
    /// The consecutive streak and last sample always start at 0 for a new activation; only
    /// `total_frame_failures` and `max_eval_micros` carry across reactivations and process runs.
    /// An unknown digest simply starts clean.
    fn seed_persisted_health(&mut self, digest: &str) {
        self.health.reset();
        if let Some(stored) = self
            .persisted_health
            .iter()
            .find(|entry| entry.digest == digest)
        {
            self.health.total_frame_failures = stored.total_frame_failures;
            self.health.max_eval_micros = stored.max_eval_micros;
        }
    }

    /// Record the active module's current cumulative failures and high-water evaluation time.
    ///
    /// Called only when one of those persisted fields changed. A digest already in the store is
    /// moved to the newest end (the reloaded order then matches the most recent activity) and an
    /// unknown digest is inserted, evicting the oldest entry at [`MAX_PERSISTED_REVISIONS`]. The
    /// manifest revision is advisory and bounded: a label the loader would reject (empty, longer
    /// than [`MAX_PERSISTED_REVISION_BYTES`] or containing control characters) is stored as
    /// `"unknown"`, so the digest-keyed entry survives a reload instead of being silently dropped.
    /// The high-water mark is capped at `i64::MAX` so it remains a valid TOML integer; the live
    /// in-memory timing itself is not clamped.
    fn sync_persisted_health(&mut self, digest: &str, revision: &str) {
        if let Some(index) = self
            .persisted_health
            .iter()
            .position(|entry| entry.digest == digest)
        {
            self.persisted_health.remove(index);
        }
        if self.persisted_health.len() == MAX_PERSISTED_REVISIONS {
            self.persisted_health.pop_front();
        }
        self.persisted_health.push_back(PersistedRevisionHealth {
            digest: digest.to_owned(),
            revision: accepted_revision_label(revision)
                .unwrap_or("unknown")
                .to_owned(),
            total_frame_failures: self.health.total_frame_failures,
            max_eval_micros: self.health.max_eval_micros.min(i64::MAX as u64),
        });
        self.state_dirty = true;
    }

    /// Reset the live movement health record for a newly committed digest and seed its cumulative
    /// values from the bounded movement store.
    ///
    /// The consecutive streak, last sample and last refusal always start at 0/`None` for a new
    /// activation; only `total_frame_failures` and `max_eval_micros` carry across reactivations and
    /// process runs. An unknown digest simply starts clean.
    fn seed_persisted_movement_health(&mut self, digest: &str) {
        self.movement_health.reset();
        if let Some(stored) = self
            .persisted_movement_health
            .iter()
            .find(|entry| entry.digest == digest)
        {
            self.movement_health.total_frame_failures = stored.total_frame_failures;
            self.movement_health.max_eval_micros = stored.max_eval_micros;
        }
    }

    /// Record the active movement processor's current cumulative failures and high-water
    /// evaluation time. Called only when one of those persisted fields changed; the digest-keyed
    /// entry is moved to the newest end and the oldest is evicted at [`MAX_PERSISTED_REVISIONS`].
    /// The revision label is bounded exactly as for the pose store, and the high-water mark is
    /// capped at `i64::MAX` so it remains a valid TOML integer.
    fn sync_persisted_movement_health(&mut self, digest: &str, revision: &str) {
        if let Some(index) = self
            .persisted_movement_health
            .iter()
            .position(|entry| entry.digest == digest)
        {
            self.persisted_movement_health.remove(index);
        }
        if self.persisted_movement_health.len() == MAX_PERSISTED_REVISIONS {
            self.persisted_movement_health.pop_front();
        }
        self.persisted_movement_health
            .push_back(PersistedRevisionHealth {
                digest: digest.to_owned(),
                revision: accepted_revision_label(revision)
                    .unwrap_or("unknown")
                    .to_owned(),
                total_frame_failures: self.movement_health.total_frame_failures,
                max_eval_micros: self.movement_health.max_eval_micros.min(i64::MAX as u64),
            });
        self.state_dirty = true;
    }

    /// The bounded document written by [`Self::persist_state`].
    ///
    /// Empty health, movement-health or stamps sections are omitted instead of serialized as
    /// empty tables; the policy is always written so a reload keeps the caller's thresholds rather
    /// than silently reverting to the default.
    fn state_document(&self) -> PatchStateFile {
        let revisions: Vec<PersistedRevisionHealth> =
            self.persisted_health.iter().cloned().collect();
        let movement_revisions: Vec<PersistedRevisionHealth> = self
            .persisted_movement_health
            .iter()
            .cloned()
            .collect();
        let pose = self
            .pose_failure_stamp
            .as_ref()
            .and_then(|stamp| persisted_stamp(stamp, self.observed.as_deref()));
        let movement = self
            .movement_failure_stamp
            .as_ref()
            .and_then(|stamp| persisted_stamp(stamp, self.movement_observed.as_deref()));
        let stamps = if pose.is_none() && movement.is_none() {
            None
        } else {
            Some(PersistedStamps { pose, movement })
        };
        PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: Some(PersistedPolicy {
                max_consecutive_frame_failures: self.health_policy.max_consecutive_frame_failures,
                max_total_frame_failures: self.health_policy.max_total_frame_failures,
            }),
            health: (!revisions.is_empty()).then_some(PersistedHealth { revisions }),
            movement_health: (!movement_revisions.is_empty())
                .then_some(PersistedHealth {
                    revisions: movement_revisions,
                }),
            stamps,
        }
    }

    /// Merge the persisted digest quarantines from `<root>/quarantine.toml` into both stores.
    ///
    /// Returns the number of digests this call inserted (entries already held, invalid entries and
    /// entries beyond the [`MAX_QUARANTINED_DIGESTS`] window are not counted). A missing file is
    /// `Ok(0)`; any other read error or a malformed/unknown-field TOML document returns `Err` and
    /// leaves both stores unchanged, because the whole document is parsed before the first entry is
    /// merged. Valid entries are merged through
    /// [`Self::quarantine_digest`]/[`Self::quarantine_movement_digest`], so the cap, dedupe and
    /// oldest-first eviction are exactly the in-memory rules; loaded entries append after whatever
    /// the running host already holds, so an existing digest stays older and is evicted first.
    /// Invalid entries are skipped silently (see [`quarantine_entry_digest`]), and loading the same
    /// file twice inserts nothing the second time, including a list longer than the cap.
    ///
    /// Loading is advisory state hygiene, not authorization: an entry only suppresses a digest at
    /// the existing pre-load gates and is never treated as a publication, revision or filesystem
    /// path. The host is the only writer of this file; a future field added by a newer binary makes
    /// `deny_unknown_fields` reject the document here, which fails open (empty stores for this run)
    /// rather than loading a partially understood store.
    pub fn load_persistent_quarantine(&mut self) -> Result<usize> {
        let path = self.root.join(QUARANTINE_FILE);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.len() <= MAX_QUARANTINE_FILE_BYTES,
            "quarantine store exceeds {MAX_QUARANTINE_FILE_BYTES} bytes"
        );
        let text = std::fs::read_to_string(&path).context("read quarantine store")?;
        let file: QuarantineFile = toml::from_str(&text).context("parse quarantine store")?;
        let mut added = 0;
        for digest in accepted_quarantine_digests(&file.pose) {
            if self.is_quarantined(&digest) {
                continue;
            }
            self.quarantine_digest(&digest);
            added += 1;
        }
        for digest in accepted_quarantine_digests(&file.movement) {
            if self.is_movement_quarantined(&digest) {
                continue;
            }
            self.quarantine_movement_digest(&digest);
            added += 1;
        }
        Ok(added)
    }

    /// Write both bounded digest stores, oldest first, to `<root>/quarantine.toml`.
    ///
    /// The file contains only the two digest lists: no library path, revision or OS error text.
    /// The document is written to `quarantine.toml.tmp`, flushed, and then renamed over the target,
    /// which replaces it on both Unix and Windows (`std::fs::rename` uses `MOVEFILE_REPLACE_EXISTING`
    /// on Windows, as the crate's `local_state::write_atomic_bytes` already relies on). No delete
    /// window exists: a reader sees either the previous complete file or the new one, and a
    /// failure before the rename leaves the previous file intact with only an inert temp behind,
    /// which this function removes best-effort. The file contents are synced, but the directory
    /// entry is not, so this is atomic replacement, not a power-loss durability guarantee.
    ///
    /// An empty store writes nothing: a fresh install never creates an empty file at 1 Hz, and an
    /// unreadable store stays on disk for diagnosis until a digest is actually quarantined. Once
    /// either list is non-empty the file is rewritten on every 1 Hz poll call (digest-only, capped
    /// at 32 + 32 strings), which is the accepted write amplification of the unconditional poll
    /// wiring; nothing is written per frame.
    pub fn persist_quarantine(&mut self) -> Result<()> {
        if self.quarantined_digests.is_empty() && self.movement_quarantined_digests.is_empty() {
            return Ok(());
        }
        let file = QuarantineFile {
            pose: self.quarantined_digests.iter().cloned().collect(),
            movement: self.movement_quarantined_digests.iter().cloned().collect(),
        };
        let text = toml::to_string(&file).context("serialize quarantine store")?;
        let target = self.root.join(QUARANTINE_FILE);
        let temp = self.root.join(QUARANTINE_TEMP);
        let written = write_quarantine_temp(&self.root, &temp, &text)
            .and_then(|()| std::fs::rename(&temp, &target).context("replace quarantine store"));
        if written.is_err() {
            // Best effort: a leftover temp file is inert and is overwritten by the next save.
            let _ = std::fs::remove_file(&temp);
        } else if self.sentinel_recovery_pending.is_some() {
            // The recovered digest is now durable. Remove the marker only when it is still the
            // exact identity this host accepted; a foreign replacement or malformed file is
            // retained for the next launch/diagnosis. Keep the failure in the sentinel slot so it
            // is not misreported as a quarantine-save failure.
            self.sentinel_recovery_pending = None;
            if let Err(error) = self.clear_owned_active_sentinel_if_matching() {
                self.sentinel_error = Some(format!(
                    "active sentinel clear after recovery: {error:#}"
                ));
            }
        }
        written
    }

    /// Replace the policy, persisted per-revision health, both artifact stamps and the movement
    /// per-revision store from `<root>/patch-state.toml`.
    ///
    /// A missing file is `Ok(0)`. The document is read with a [`MAX_PATCH_STATE_BYTES`] cap and
    /// parsed in full before any host state changes, so an oversized, unreadable, malformed,
    /// unknown-field or unsupported-schema file returns `Err` with the policy, stores, stamps and
    /// live health untouched. Once the document parses, each entry is validated independently:
    /// an invalid digest, revision label, library basename or oversized artifact length skips that
    /// entry (and clears that seam's stamp) without failing the rest of the document. A movement
    /// entry is validated by the same helpers and skipped without affecting the pose store. The
    /// return value counts the per-revision entries actually applied across both stores, not
    /// skipped ones.
    ///
    /// On success `persisted_health` and `persisted_movement_health` replace (not merge with) their
    /// in-memory stores, `health_policy` replaces the current pose policy, both
    /// `*_failure_stamp` slots replace their current values and the stored latched manifest text
    /// replaces `observed`/`movement_observed`, and `state_dirty` is cleared because memory and
    /// disk now agree. Live health counters are not touched: the next activation of a stored pose
    /// digest seeds its cumulative total and high-water mark through
    /// [`Self::seed_persisted_health`], and the next movement commit does the same through
    /// [`Self::seed_persisted_movement_health`]. The movement demotion policy is a live host
    /// override and is not serialized in this slice.
    ///
    /// Loading is advisory state hygiene, not authorization. A persisted digest only seeds
    /// counters for a later verified activation of that same content identity; it never selects,
    /// opens or quarantines an artifact. A stamp only suppresses a same-text re-read until the
    /// artifact's length or modification time changes, and no digest or OS error text is trusted
    /// from the file. The file is single-client scope like `quarantine.toml`: a fixed temp name,
    /// no interprocess lock and last-writer-wins replacement, so a root shared by concurrent
    /// clients is unsupported.
    pub fn load_persistent_state(&mut self) -> Result<usize> {
        let path = self.root.join(PATCH_STATE_FILE);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.len() <= MAX_PATCH_STATE_BYTES,
            "patch state exceeds {MAX_PATCH_STATE_BYTES} bytes"
        );
        let text = std::fs::read_to_string(&path).context("read patch state")?;
        let file: PatchStateFile = toml::from_str(&text).context("parse patch state")?;
        ensure!(
            file.schema_version == PATCH_STATE_SCHEMA_VERSION,
            "unsupported patch state schema version {}",
            file.schema_version
        );
        let policy = file.policy.map_or_else(PatchHealthPolicy::default, |policy| {
            PatchHealthPolicy {
                max_consecutive_frame_failures: policy.max_consecutive_frame_failures,
                max_total_frame_failures: policy.max_total_frame_failures,
            }
        });
        let revisions = file
            .health
            .map_or_else(VecDeque::<PersistedRevisionHealth>::new, |health| {
                accepted_persisted_revisions(&health.revisions)
            });
        let movement_revisions = file.movement_health.map_or_else(
            VecDeque::<PersistedRevisionHealth>::new,
            |health| accepted_persisted_revisions(&health.revisions),
        );
        let (pose_stamp, pose_latch, movement_stamp, movement_latch) = match file.stamps {
            Some(stamps) => {
                let (pose_stamp, pose_latch) = stamps
                    .pose
                    .and_then(accepted_persisted_stamp)
                    .map_or((None, None), |(stamp, latch)| {
                        (Some(stamp), Some(latch))
                    });
                let (movement_stamp, movement_latch) = stamps
                    .movement
                    .and_then(accepted_persisted_stamp)
                    .map_or((None, None), |(stamp, latch)| {
                        (Some(stamp), Some(latch))
                    });
                (
                    pose_stamp,
                    pose_latch,
                    movement_stamp,
                    movement_latch,
                )
            }
            None => (None, None, None, None),
        };
        let applied = revisions.len() + movement_revisions.len();
        self.health_policy = policy;
        self.persisted_health = revisions;
        self.persisted_movement_health = movement_revisions;
        self.pose_failure_stamp = pose_stamp;
        self.observed = pose_latch;
        self.movement_failure_stamp = movement_stamp;
        self.movement_observed = movement_latch;
        self.state_dirty = false;
        Ok(applied)
    }

    /// Write the bounded policy, per-revision pose/movement health and artifact stamps to
    /// `<root>/patch-state.toml`, returning `Ok(true)` when the file was replaced.
    ///
    /// When `state_dirty` is `false` this is `Ok(false)` and the file is not rewritten: a healthy
    /// steady state with unchanged high-water marks does not write at 1 Hz. Otherwise the document
    /// is serialized and size-checked, written to `patch-state.toml.tmp` (create/truncate,
    /// `write_all`, `sync_all`) and renamed over the target in one `std::fs::rename`, the same
    /// atomic-replacement pattern as [`Self::persist_quarantine`] (no delete-then-rename window;
    /// the directory entry is not synced, so this is atomic replacement, not a power-loss
    /// guarantee). A failure before the rename leaves the previous file byte-identical, removes the
    /// inert temp best-effort, leaves `state_dirty` set and changes no in-memory state, so the next
    /// poll retries. A failed save never stops the poll or either seam.
    pub fn persist_state(&mut self) -> Result<bool> {
        if !self.state_dirty {
            return Ok(false);
        }
        let text = toml::to_string(&self.state_document()).context("serialize patch state")?;
        ensure!(
            text.len() as u64 <= MAX_PATCH_STATE_BYTES,
            "patch state serialization exceeds {MAX_PATCH_STATE_BYTES} bytes"
        );
        let target = self.root.join(PATCH_STATE_FILE);
        let temp = self.root.join(PATCH_STATE_TEMP);
        if let Err(error) = write_state_temp(&self.root, &temp, &text)
            .and_then(|()| std::fs::rename(&temp, &target).context("replace patch state"))
        {
            // Best effort: a leftover temp file is inert and is overwritten by the next save.
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
        self.state_dirty = false;
        Ok(true)
    }

    /// Write a pose/movement crash sentinel for a verified publication to
    /// `<root>/active-sentinel.toml`.
    ///
    /// The document holds only the digest identity of the pose or movement module that was being
    /// loaded or evaluated as a staged canary when the process died: `{ revision, sha1, kind,
    /// stage }`. There is no library path and no OS error text, so the file can never be joined to
    /// a path or mistaken for a diagnosis; it is a loading/canary marker, not a manifest and never
    /// selects code to load.
    ///
    /// `sha1` is validated and normalized through [`quarantine_entry_digest`], so only an exactly
    /// 40 ASCII hex digit content identity is written. The document is written to
    /// `active-sentinel.toml.tmp` (create, `write_all`, `sync_all`) and then renamed over the target
    /// in one `std::fs::rename`, which replaces an existing file on both Unix and Windows with no
    /// delete-then-rename window; a failure before the rename leaves the previous file intact with
    /// only an inert temp behind, which this function removes best-effort. The contents are flushed;
    /// the directory entry is not, so this is atomic replacement, not a power-loss durability
    /// guarantee. This covers the pose loading/staged-canary windows and the movement
    /// loading/staged-canary windows.
    ///
    /// Best effort: callers (`Host::poll`, `Host::evaluate`, `Host::stage_movement_processor` and
    /// `Host::movement_proposal`) treat a failure here as a [`Self::sentinel_error`] and still
    /// perform the module load/evaluation, because an unwritable sentinel must never fail or skip
    /// the work it was written for.
    fn write_active_sentinel(
        &mut self,
        kind: SentinelKind,
        revision: &str,
        sha1: &str,
        stage: SentinelStage,
    ) -> Result<()> {
        if self.sentinel_recovery_pending.is_some() {
            // A recovered digest is awaiting its quarantine commit. Commit it and clear the old
            // marker before writing a replacement; on failure keep the old marker and let the
            // caller proceed without a new marker rather than lose the recovered digest.
            self.persist_quarantine()
                .context("commit pending sentinel recovery before a new load")?;
        }
        let Some(sha1) = quarantine_entry_digest(sha1) else {
            bail!("active sentinel digest is not a 40-hex content identity");
        };
        let file = ActiveSentinelFile {
            revision: revision.to_owned(),
            sha1: sha1.clone(),
            kind,
            stage,
        };
        let text = toml::to_string(&file).context("serialize active sentinel")?;
        ensure!(
            text.len() as u64 <= MAX_ACTIVE_SENTINEL_BYTES,
            "active sentinel serialization exceeds {MAX_ACTIVE_SENTINEL_BYTES} bytes"
        );
        let target = self.root.join(ACTIVE_SENTINEL_FILE);
        let temp = self.root.join(ACTIVE_SENTINEL_TEMP);
        let written = write_sentinel_temp(&self.root, &temp, &text)
            .and_then(|()| std::fs::rename(&temp, &target).context("replace active sentinel"));
        if written.is_err() {
            // Best effort: a leftover temp file is inert and is overwritten by the next write.
            let _ = std::fs::remove_file(&temp);
        } else {
            // Own exactly the bytes that were published. A later writer that replaces the file
            // with a different identity no longer matches this value.
            self.sentinel_marker_owned = Some(file);
        }
        written
    }

    /// Clear the pose/movement crash sentinel. `NotFound` is the success case (nothing to
    /// clear), any other removal error is returned so the caller can surface it through
    /// [`Self::sentinel_error`]. Call-scoped operations call this after writing a marker they own;
    /// the recovery-commit and clean-exit paths use the identity-checked
    /// [`Self::clear_owned_active_sentinel_if_matching`] instead. Startup recovery does not call
    /// either path directly.
    pub fn clear_active_sentinel(&mut self) -> Result<()> {
        let path = self.root.join(ACTIVE_SENTINEL_FILE);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.sentinel_marker_owned = None;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.sentinel_marker_owned = None;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Remove `active-sentinel.toml` only when it still matches the exact identity this host last
    /// wrote or accepted. Used by the recovery commit and clean-shutdown paths so a foreign
    /// replacement is never deleted. Returns `Ok(true)` only when ownership is `Some` and the
    /// marker is absent or was removed; `Ok(false)` when ownership is `None` or the file is
    /// oversized/malformed or differs from the owned identity. A missing owned file drops the
    /// ownership record.
    fn clear_owned_active_sentinel_if_matching(&mut self) -> Result<bool> {
        let Some(owned) = self.sentinel_marker_owned.clone() else {
            return Ok(false);
        };
        let path = self.root.join(ACTIVE_SENTINEL_FILE);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.sentinel_marker_owned = None;
                return Ok(true);
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.len() > MAX_ACTIVE_SENTINEL_BYTES {
            // Do not read an oversized replacement; retain it for the next launch/diagnosis.
            return Ok(false);
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.sentinel_marker_owned = None;
                return Ok(true);
            }
            Err(error) => return Err(error.into()),
        };
        let Ok(file) = toml::from_str::<ActiveSentinelFile>(&text) else {
            // A malformed replacement is not ours to delete.
            return Ok(false);
        };
        if file != owned {
            return Ok(false);
        }
        self.clear_active_sentinel()?;
        Ok(true)
    }

    /// Clean-exit safety net for a stale `active-sentinel.toml`.
    ///
    /// A normal client exit must not leave a stale marker for the next launch, because a stale
    /// marker wrongly quarantines a healthy digest. This method clears that stale marker on a clean
    /// shutdown, but it is **not** a substitute for the call-scoped clear that runs after every
    /// regular load/proposal, and it is **not** a crash path: it must never be invoked to recover
    /// from a native fault.
    ///
    /// If [`Self::sentinel_recovery_pending`] is `Some`, the marker is the only durable record of a
    /// digest recovered at startup whose `quarantine.toml` commit has not succeeded. In that case
    /// this method returns `Ok(false)` immediately and leaves the marker untouched; only
    /// [`Self::persist_quarantine`] may clear it. Otherwise ownership is decided by content
    /// identity: [`Self::sentinel_marker_owned`] holds the exact [`ActiveSentinelFile`] this host
    /// last wrote or accepted. When it is `Some`, the marker on disk is read and parsed and the
    /// file is removed only when it still matches that identity. A marker replaced by another
    /// writer (different revision/sha1/kind/stage), an unparseable file, or a marker this host never
    /// wrote or accepted is retained (`Ok(false)`) for the next launch/diagnosis. A missing file is
    /// a successful clean state (`Ok(true)`), and read errors are returned to the caller.
    ///
    /// Errors are returned to the caller and are never recorded in [`Self::sentinel_error`]; the
    /// method does not populate the poll slot.
    pub fn clear_active_sentinel_on_clean_shutdown(&mut self) -> Result<bool> {
        if self.sentinel_recovery_pending.is_some() {
            return Ok(false);
        }
        if self.sentinel_marker_owned.is_none() {
            // The marker was not written by this host and was not accepted by
            // `load_active_sentinel`; it may be an unvalidated previous-run marker, a malformed
            // file, or another process's marker. Leave it for the next launch/diagnosis. A missing
            // file is still a successful clean state; a metadata error is returned rather than
            // silently treated as absent.
            return match std::fs::metadata(self.root.join(ACTIVE_SENTINEL_FILE)) {
                Ok(_) => Ok(false),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
                Err(error) => Err(error.into()),
            };
        }
        // Ownership is content-bound: a later writer may have replaced this host's marker. Only the
        // exact identity this host published or accepted may be deleted here; a foreign or
        // unparseable file must survive for the next launch/diagnosis.
        self.clear_owned_active_sentinel_if_matching()
    }

    /// Load a pose/movement crash sentinel written by a previous launch that died during a pose
    /// loading window (`Library::new`/entry/header validation), the staged pose candidate's canary
    /// evaluation window, the movement loading window (`MovementLoaded::open`/`Library::new`), or
    /// the first staged movement proposal's `process` window.
    ///
    /// Returns `Ok(None)` when no sentinel file is present (no crash marker). A missing file is the
    /// normal clean state; both digest stores are untouched. A malformed document, an unknown
    /// field, a digest that is not exactly 40 ASCII hex digits, or an unsupported kind/stage
    /// combination is an error: the file is left in place and both digest stores stay unchanged
    /// (fail-safe),
    /// because a recoverable marker must never be silently overwritten from unvalidated state. A
    /// `Pose` marker with stage `loading` or `canary` is recovered by adding its normalized digest
    /// to the bounded pose quarantine; a `Movement` marker with stage `loading` or `canary` is
    /// recovered into the bounded movement quarantine (both through the existing dedupe/cap
    /// helpers), **without removing the marker**; the caller must persist the quarantine first, and
    /// [`Self::persist_quarantine`] clears the marker only when it still matches the accepted
    /// identity, so a crash or persist failure between recovery and commit leaves the marker for
    /// the next launch. Returns `Some(digest)`.
    /// Unknown stage strings remain TOML parse errors.
    ///
    /// The crash window itself (the fault inside `Library::new` before the clear runs, or inside
    /// `process()` before the canary clear runs) cannot be exercised by a test. The regular path is
    /// verified by the observing probe, which parses the canary marker during the module call and
    /// asserts it is gone afterwards; the native-fault path is verified by inspection of the
    /// write-before-call/clear-after-return ordering in [`Self::poll`] and [`Self::evaluate`].
    pub fn load_active_sentinel(&mut self) -> Result<Option<String>> {
        // Any load attempt drops this host's ownership first: a failed re-read must not inherit
        // ownership from an earlier accepted marker, or clean shutdown could delete bytes that were
        // never validated. A valid marker re-arms ownership at the end of this call.
        self.sentinel_marker_owned = None;
        let path = self.root.join(ACTIVE_SENTINEL_FILE);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.len() <= MAX_ACTIVE_SENTINEL_BYTES,
            "active sentinel exceeds {MAX_ACTIVE_SENTINEL_BYTES} bytes"
        );
        let text = std::fs::read_to_string(&path).context("read active sentinel")?;
        let file: ActiveSentinelFile = toml::from_str(&text).context("parse active sentinel")?;
        let Some(sha1) = quarantine_entry_digest(&file.sha1) else {
            return Err(anyhow::anyhow!(
                "active sentinel digest is not a 40-hex content identity"
            ));
        };
        let digest = sha1;
        match file.kind {
            SentinelKind::Pose => {
                if file.stage != SentinelStage::Loading && file.stage != SentinelStage::Canary {
                    return Err(anyhow::anyhow!(
                        "active sentinel stage is not an implemented pose window"
                    ));
                }
                self.quarantine_digest(&digest);
            }
            SentinelKind::Movement => {
                if file.stage != SentinelStage::Loading && file.stage != SentinelStage::Canary {
                    return Err(anyhow::anyhow!(
                        "active sentinel stage is not an implemented movement window"
                    ));
                }
                self.quarantine_movement_digest(&digest);
            }
        }
        self.sentinel_recovery_pending = Some(digest.clone());
        self.sentinel_marker_owned = Some(file);
        Ok(Some(digest))
    }

    /// Drain the best-effort pose/movement crash-sentinel error slot. A failed write or clear
    /// during `poll`, `evaluate`, movement staging or a staged movement proposal sets it; a module
    /// load/evaluation is never failed or skipped because the sentinel could not be written.
    pub fn take_sentinel_error(&mut self) -> Option<String> {
        self.sentinel_error.take()
    }

    /// Poll only a complete publication. A failed candidate never replaces the active module.
    ///
    /// After `verify_publication` proved the bytes, the host writes the pose loading-window crash
    /// sentinel (`<root>/active-sentinel.toml`) before the irreversible `Loaded::open`, and clears it
    /// immediately after the open returns, so a native fault inside the load leaves a marker for the
    /// next launch while a regular failure leaves none. Sentinel write/clear failures are best
    /// effort and surface through [`Self::sentinel_error`].
    pub fn poll(&mut self) -> Result<bool> {
        self.publication_reason = None;
        let text = match std::fs::read_to_string(self.root.join("ready.toml")) {
            Ok(text) => text,
            Err(error) => {
                // A staged DLL may be invoked later, after a rig becomes available.
                // Only keep it staged while its publication can still be observed.
                self.pending = None;
                self.observed = None;
                self.set_pose_failure_stamp(None);
                return if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(false)
                } else {
                    let reason = if error.kind() == std::io::ErrorKind::InvalidData {
                        PatchRejectReason::ManifestUnreadable
                    } else {
                        PatchRejectReason::TransientIoPending
                    };
                    self.publication_reason = Some(reason);
                    Err(error.into())
                };
            }
        };
        if self.observed.as_ref() == Some(&text) {
            // A terminal failure latches the manifest text. If the artifact it named changed on
            // disk without the text changing (a manual repair or an in-place re-copy), the stamp
            // no longer matches, so drop the latch and retry the publication once. A missing or
            // unreadable artifact also mismatches. No stamp (malformed TOML or a digest refusal)
            // keeps the old quiet `Ok(false)`.
            if self
                .pose_failure_stamp
                .as_ref()
                .is_some_and(|stamp| !stamp.matches_current(&self.root))
            {
                self.observed = None;
                self.set_pose_failure_stamp(None);
            } else {
                return Ok(false);
            }
        }
        // Any changed manifest invalidates the previous observation and any superseded staged
        // candidate so that reverting after a failed replacement re-processes the prior manifest.
        self.pending = None;
        self.observed = None;
        self.rejection = None;
        self.rejection_reason = None;
        self.set_pose_failure_stamp(None);
        let publication: Publication = match toml::from_str(&text) {
            Ok(publication) => publication,
            Err(error) => {
                // A malformed manifest is deterministic content. Latch it so the 1 Hz poll does
                // not re-parse and re-report the same bytes; a changed manifest clears the latch
                // above and is retried. There is no trustworthy publication, so no artifact stamp
                // is recorded; the typed rejection reason is recorded below.
                self.set_pose_failure_stamp(None);
                self.observed = Some(text);
                self.publication_reason = Some(PatchRejectReason::ManifestMalformed);
                return Err(error.into());
            }
        };
        if self.is_quarantined(&publication.sha1) {
            // A digest rejected for a deterministic content/contract failure is refused before the
            // active-module shortcut or any DLL read/`Library::new`, even when a changed manifest
            // (`B -> C -> B`) names it again. Latch this text so the unchanged manifest is a quiet
            // `Ok(false)`; a genuinely new digest is still evaluated. The digest is the refusal key,
            // so no artifact stamp is recorded: replacing the bytes must not spam the gate.
            let digest = publication.sha1.clone();
            self.set_pose_failure_stamp(None);
            self.observed = Some(text);
            self.publication_reason = Some(PatchRejectReason::Quarantined);
            bail!(
                "pose module digest {digest} is quarantined after a terminal load or evaluation failure"
            );
        }
        if self.active.as_ref().is_some_and(|m| {
            !publication.revision.is_empty()
                && m.digest == publication.sha1
                && m.filename == publication.library
        }) {
            self.observed = Some(text);
            return Ok(false);
        }
        // Verify the declared digest against the bytes before loading. A verify failure latches the
        // text but cannot quarantine `publication.sha1`: a digest mismatch or a pre-read size/name
        // failure never proved that string is the artifact's identity, and quarantining it could
        // poison the active module's digest or a legitimate same-digest repair.
        //
        // Capture the attempted artifact's metadata *before* verification reads it. Capturing after
        // a failed read could describe a publisher's just-completed repair and make the next
        // identical-text poll look unchanged, masking the very repair this stamp detects. A change
        // between this stat and the read can only cause one extra retry, never a masked repair.
        let attempt_stamp = artifact_stamp(&self.root, &publication.library);
        let (path, digest) = match verify_publication(&self.root, &publication, ModuleKind::Pose) {
            Ok(verified) => verified,
            Err(error) => {
                if transient_io(&error) || publish_in_progress(&self.root) {
                    self.publication_reason = Some(PatchRejectReason::TransientIoPending);
                    return Err(error);
                }
                // Terminal verification failure: record the attempted artifact identity so a later
                // same-text repair with new bytes is retried instead of being masked by the latch.
                self.publication_reason = Some(
                    publication_reason(&error).unwrap_or(PatchRejectReason::ArtifactRejected),
                );
                self.set_pose_failure_stamp(attempt_stamp.clone());
                self.observed = Some(text);
                return Err(error);
            }
        };
        // The digest is now verified against the bytes. A terminal post-verification failure is a
        // deterministic artifact problem, so quarantine it before latching the manifest.
        let quarantine_digest = digest.clone();
        // Pose loading-window sentinel: the bytes are now verified, so mark the pose module as
        // loading before the irreversible `Library::new`. A native fault inside the open (entry,
        // header, processor checks) then leaves this marker for the next launch to quarantine.
        // A failure here is best effort: it never fails or skips the module load. Clear only when
        // this load actually wrote a marker; a failed write may have intentionally retained a
        // pending-recovery marker, which must survive for the next launch.
        let sentinel_written = match self.write_active_sentinel(
            SentinelKind::Pose,
            &publication.revision,
            &publication.sha1,
            SentinelStage::Loading,
        ) {
            Ok(()) => true,
            Err(error) => {
                self.sentinel_error = Some(format!("active sentinel write: {error:#}"));
                false
            }
        };
        let open_result = Loaded::open(path, digest, publication);
        // Immediately after the load attempt returns, clear the loading marker this call wrote: an
        // `Ok` means the module was accepted for staging, an `Err` means a regular failure
        // (transient handled below) or a deterministic content/contract error. Only a process
        // fault before this clear runs can leave a new marker behind.
        if sentinel_written {
            if let Err(error) = self.clear_active_sentinel() {
                self.sentinel_error = Some(format!("active sentinel clear: {error:#}"));
            }
        }
        let pending = match open_result {
            Ok(pending) => pending,
            Err(error) => {
                if transient_io(&error) || publish_in_progress(&self.root) {
                    // Leave `observed` unlatched so an incomplete copy is retried unchanged.
                    self.publication_reason = Some(PatchRejectReason::TransientIoPending);
                    return Err(error);
                }
                self.publication_reason =
                    Some(publication_reason(&error).unwrap_or(PatchRejectReason::LoadFailed));
                self.quarantine_digest(&quarantine_digest);
                self.set_pose_failure_stamp(attempt_stamp);
                self.observed = Some(text);
                return Err(error);
            }
        };
        self.pending = Some(pending);
        // A successful open latches the manifest even though promotion happens on the next batch.
        self.set_pose_failure_stamp(None);
        self.observed = Some(text);
        Ok(true)
    }

    pub fn has_module(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn revision(&self) -> Option<&str> {
        self.active.as_ref().map(|m| m.revision.as_str())
    }
    /// Verified content identity of the currently active library.
    pub fn digest(&self) -> Option<&str> {
        self.active.as_ref().map(|m| m.digest.as_str())
    }

    /// Read-only health of the active pose module.
    ///
    /// `revision`/`digest` name the module active at snapshot time. After a demotion with no valid
    /// predecessor the slot is empty, so both are `None` while the last counters/timings remain
    /// visible until the next activation resets them; the failed identity is available from
    /// `take_active_failure`/`take_restore_outcomes`. A restored predecessor starts a new
    /// activation: the consecutive streak restarts at 0, while the cumulative total and high-water
    /// evaluation time are seeded from `patch-state.toml`. `quarantined_digests` is the current
    /// pose quarantine length. This returns the live activation counters; the persisted
    /// per-revision store is written separately by [`Self::persist_state`].
    pub fn health_snapshot(&self) -> PatchHealthSnapshot {
        PatchHealthSnapshot {
            revision: self.active.as_ref().map(|module| module.revision.clone()),
            digest: self.active.as_ref().map(|module| module.digest.clone()),
            consecutive_frame_failures: self.health.consecutive_frame_failures,
            total_frame_failures: self.health.total_frame_failures,
            last_eval_micros: self.health.last_eval_micros,
            max_eval_micros: self.health.max_eval_micros,
            quarantined_digests: self.quarantined_digests.len(),
        }
    }

    /// Replace the demotion thresholds. Affects only failures recorded after this call;
    /// existing counters are kept, so relaxing a policy lets a module that already failed remain
    /// active until the next failure. [`PatchHealthPolicy::default`] is single-strike (1/3). A
    /// changed policy marks the persisted state dirty; [`Self::persist_state`] writes it on the
    /// next poll.
    pub fn set_health_policy(&mut self, policy: PatchHealthPolicy) {
        if self.health_policy != policy {
            self.health_policy = policy;
            self.state_dirty = true;
        }
    }

    /// The demotion thresholds currently in force for active pose-module failures.
    pub fn health_policy(&self) -> PatchHealthPolicy {
        self.health_policy
    }

    pub fn take_rejection(&mut self) -> Option<String> {
        // Keep the string-taking API for existing logs/tests, but never leave the paired typed
        // reason behind for a later detail-taking call to observe as stale state.
        self.rejection_reason = None;
        self.rejection.take()
    }

    /// Drain the most recent candidate-rejection detail and its fine typed reason together.
    ///
    /// Both slots are cleared even when one is absent; a missing typed reason degrades to
    /// [`PatchEvaluationReason::Unknown`] so an older/untyped rejection is still reportable.
    pub fn take_rejection_detail(&mut self) -> Option<(String, PatchEvaluationReason)> {
        let reason = self
            .rejection_reason
            .take()
            .unwrap_or(PatchEvaluationReason::Unknown);
        self.rejection.take().map(|detail| (detail, reason))
    }

    /// Drain the typed reason for the most recent pose publication poll rejection.
    ///
    /// Set by [`Self::poll`] on every error path and cleared at the start of each poll. A quiet
    /// `Ok(false)` (unchanged latch, active shortcut or no manifest) leaves it `None`.
    pub fn take_publication_reason(&mut self) -> Option<PatchRejectReason> {
        self.publication_reason.take()
    }

    /// Enable or disable a hot-pluggable gameplay seam. `Pose` is always enabled and unaffected.
    /// Disabling the movement seam evicts its staged/active processor immediately but keeps the
    /// manifest latch so retention still pins the publication; enabling it clears the latch so a
    /// publication made while disabled can be staged.
    pub fn set_seam_enabled(&mut self, kind: ModuleKind, enabled: bool) {
        let was_enabled = self.is_seam_enabled(kind);
        self.seams.set_seam_enabled(kind, enabled);
        if kind == ModuleKind::Movement {
            if enabled && !was_enabled {
                // Let the next stage call publish the manifest that was refused while disabled.
                self.movement_proposal_ready = false;
                self.movement_canary_attempted = false;
                self.movement_observed = None;
                self.set_movement_failure_stamp(None);
                self.movement_refusal = None;
                self.movement_rejection = None;
            } else if !enabled && was_enabled {
                // A disabled seam must not keep running or promote a movement processor. Keep the
                // manifest latch so garbage collection still pins the publication; enabling clears
                // it and lets the next stage call publish the manifest again.
                self.movement_proposal_ready = false;
                self.unload_movement_processor();
                self.movement_refusal = None;
                self.movement_rejection = None;
            }
        }
    }

    /// Whether `kind`'s seam is currently enabled. The pose seam is always enabled.
    pub fn is_seam_enabled(&self, kind: ModuleKind) -> bool {
        self.seams.is_seam_enabled(kind)
    }

    pub fn take_active_failure(&mut self) -> Option<String> {
        // Clear the paired typed reason so a later detail-taking call cannot report stale state.
        self.active_failure_reason = None;
        self.active_failure.take()
    }

    /// Drain the most recent active-failure detail and its fine typed reason together.
    ///
    /// Both slots are cleared even when one is absent; a missing typed reason degrades to
    /// [`PatchEvaluationReason::Unknown`]. A below-threshold tolerated failure and a
    /// threshold-reaching demotion both populate this slot. The free-form detail may include a
    /// failing predecessor in the same restore attempt; the typed reason is always the active
    /// module's own reason, while [`RestoreOutcome::Unavailable`] carries the predecessor/outcome
    /// reason separately.
    pub fn take_active_failure_detail(&mut self) -> Option<(String, PatchEvaluationReason)> {
        let reason = self
            .active_failure_reason
            .take()
            .unwrap_or(PatchEvaluationReason::Unknown);
        self.active_failure
            .take()
            .map(|detail| (detail, reason))
    }

    /// Drain the bounded active-failure/restore outcomes accumulated by `evaluate`.
    ///
    /// Each outcome is one failed module and the fallback decision made for it, in evaluation
    /// order, so a restored module that fails again is reported separately instead of being
    /// collapsed into one terminal. No loader error text, path or revision label is included.
    pub fn take_restore_outcomes(&mut self) -> Vec<RestoreOutcome> {
        self.restores.drain(..).collect()
    }

    fn push_restore(&mut self, outcome: RestoreOutcome) {
        if self.restores.len() == MAX_RESTORE_OUTCOMES {
            self.restores.pop_front();
        }
        self.restores.push_back(outcome);
    }

    /// Only this host's digest-named artifacts are collected. A publisher uses the same lock;
    /// the active library, three predecessors and the current publication stay pinned.
    ///
    /// The sweep is anchored to both of this host's publication latches: a pose-only install
    /// behaves as before, and a movement-only install collects its own retired artifacts without
    /// requiring a pose `ready.toml`.
    pub fn collect_retired(&self) -> Result<usize> {
        if self.observed.is_none() && self.movement_observed.is_none() {
            return Ok(0);
        }
        let root = match self.root.canonicalize() {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let lock_path = root.join("publish.lock");
        let lock = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(lock) => lock,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let result = (|| {
            // A publication changing under the sweep aborts it, as before. An absent manifest is
            // equal to an unlatched publication.
            let pose_text = std::fs::read_to_string(root.join("ready.toml")).ok();
            if pose_text.as_deref() != self.observed.as_deref() {
                return Ok(0);
            }
            let movement_text = std::fs::read_to_string(root.join(MOVEMENT_MANIFEST)).ok();
            if movement_text.as_deref() != self.movement_observed.as_deref() {
                return Ok(0);
            }
            let mut published = Vec::with_capacity(2);
            for text in [pose_text.as_deref(), movement_text.as_deref()]
                .into_iter()
                .flatten()
            {
                let Ok(publication) = toml::from_str::<Publication>(text) else {
                    return Ok(0);
                };
                published.push(publication.library);
            }
            let pinned: std::collections::HashSet<String> = self
                .previous
                .iter()
                .chain(self.active.iter())
                .chain(self.pending.iter())
                .map(|m| m.filename.clone())
                .chain(
                    self.movement_pending
                        .iter()
                        .chain(self.movement_active.iter())
                        .map(|m| m.filename.clone()),
                )
                .chain(published)
                .collect();
            let mut removed = 0;
            for entry in std::fs::read_dir(&root)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                // Only the seams this host owns are collected. Other `*-<digest>.dll` artifacts
                // (a future combat/space/story publication) are left alone.
                let managed = [ModuleKind::Pose, ModuleKind::Movement]
                    .iter()
                    .any(|kind| is_managed_artifact(&name, kind.prefix()));
                if !managed || pinned.contains(&name) || !entry.file_type()?.is_file() {
                    continue;
                }
                // A direct regular-file child of the canonical module folder, never recursive.
                let path = root.join(&name);
                if path.parent() != Some(root.as_path()) {
                    continue;
                }
                match std::fs::remove_file(path) {
                    Ok(()) => removed += 1,
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(removed)
        })();
        drop(lock);
        let _ = std::fs::remove_file(lock_path);
        result
    }

    /// Stage the complete rig batch before the caller changes any live pose. Promotion happens
    /// only after all output validates. The caller's rig spans must tile the bone batch in order:
    /// a gap, overlap, reordered span or empty rig list with bones is a host/caller error and never
    /// reaches a module or quarantines a digest. All calls are synchronous; no DLL worker may escape.
    /// A staged candidate is wrapped in a best-effort `active-sentinel.toml` canary marker for the
    /// duration of its `Loaded::evaluate` call.
    pub fn evaluate(
        &mut self,
        bones: &[BoneLocal],
        rigs: &[RigSpan],
        seconds: f64,
        output: &mut Vec<BoneLocal>,
    ) -> Result<bool> {
        if bones.is_empty() {
            return Ok(false);
        }
        // Validate the host frame before touching `pending`, `active`, or `previous` so a frame
        // that fails these bounds checks never drains rollback history or reaches a module.
        if bones.len() > u32::MAX as usize || rigs.len() > u32::MAX as usize {
            bail!("pose batch exceeds u32 ABI counts");
        }
        if bones.len() > i32::MAX as usize {
            bail!("pose batch exceeds i32 parent indices");
        }
        if !seconds.is_finite() {
            bail!("pose frame time is not finite");
        }
        if !rigs.iter().all(|rig| {
            (rig.first as usize)
                .checked_add(rig.count as usize)
                .is_some_and(|end| end <= bones.len())
        }) {
            bail!("host rig span out of bounds for {} bones", bones.len());
        }
        // The spans must tile the whole batch in order. `apply_pose` appends one span per selected
        // rig, so a gap, overlap, reordered span or missing rig would hand a module a batch the
        // host never validated as a complete rig frame. This runs after the bounds check (so the
        // slice accesses below stay safe) and before any module dispatch. A zero-count span is
        // allowed when it starts at the current coverage; it advances nothing.
        let mut covered = 0usize;
        for rig in rigs {
            let first = rig.first as usize;
            ensure!(
                first == covered,
                "host rig spans must tile the batch in order: span starts at {first}, expected {covered} of {} bones",
                bones.len()
            );
            let Some(end) = first.checked_add(rig.count as usize) else {
                bail!(
                    "host rig spans must tile the batch in order: coverage overflows at {first}+{} of {} bones",
                    rig.count,
                    bones.len()
                );
            };
            ensure!(
                end <= bones.len(),
                "host rig spans must tile the batch in order: coverage {end} exceeds {} bones",
                bones.len()
            );
            covered = end;
        }
        ensure!(
            covered == bones.len(),
            "host rig spans must tile the batch in order: coverage {covered} of {} bones",
            bones.len()
        );
        // A negative parent is the root marker; a non-negative parent must name an earlier bone in
        // the same rig. The batch-wide check catches an index beyond every rig; the per-rig check
        // catches a self/forward parent or an index outside the owning rig. Both keep an
        // out-of-bounds caller frame from dispatching a module and then quarantining it for output
        // the caller caused. The spans above already tile the batch, so every bone is checked
        // inside exactly one rig.
        if !bones.iter().all(|bone| bone.parent < bones.len() as i32) {
            bail!(
                "host input bone parent is out of bounds for {} bones",
                bones.len()
            );
        }
        if !rigs.iter().all(|rig| {
            let first = rig.first as usize;
            let count = rig.count as usize;
            bones[first..first + count]
                .iter()
                .enumerate()
                .all(|(index, bone)| bone.parent < index as i32)
        }) {
            bail!("host input bone parent must precede its child within the rig");
        }
        if !bones.iter().all(BoneLocal::valid) {
            bail!("host input bone contains non-finite or zero-norm transform");
        }
        if let Some(candidate) = self.pending.take() {
            // Pose canary-window sentinel: the candidate has already been loaded and verified, so
            // mark it as in-canary before the irreversible `process()` call. A native fault during
            // that call leaves this marker for the next launch to quarantine. A failure here is
            // best effort and never skips the evaluation. Clear only when this call actually wrote
            // a marker; a failed write may have intentionally retained a pending-recovery marker.
            let sentinel_written = match self.write_active_sentinel(
                SentinelKind::Pose,
                &candidate.revision,
                &candidate.digest,
                SentinelStage::Canary,
            ) {
                Ok(()) => true,
                Err(error) => {
                    self.sentinel_error =
                        Some(format!("active sentinel write: {error:#}"));
                    false
                }
            };
            let canary_result = candidate.evaluate(bones, rigs, seconds, output);
            // Immediately after the canary call returns, clear the marker this call wrote. A
            // successful return, a regular `Err` and a panic-free completion all reach this point;
            // only a process fault, a host panic, or a failed clear can leave a new marker behind.
            // A clear failure is reported; the stale marker can quarantine a healthy digest on the
            // next launch. Do not clear a retained pending-recovery marker when the write failed.
            if sentinel_written {
                if let Err(error) = self.clear_active_sentinel() {
                    self.sentinel_error =
                        Some(format!("active sentinel clear: {error:#}"));
                }
            }
            match canary_result {
                Ok(()) => {
                    let promoted_digest = candidate.digest.clone();
                    if let Some(old) = self.active.replace(candidate) {
                        self.previous.retain(|m| m.digest != old.digest);
                        self.previous.push_back(old);
                    }
                    while self.previous.len() > 3 {
                        self.previous.pop_front();
                    }
                    // A newly promoted candidate is a new activation: the canary call is an
                    // activation probe, not a timed steady-state evaluation, so the consecutive
                    // streak and last sample start at 0 while the cumulative total and high-water
                    // mark are seeded from the per-revision store.
                    self.seed_persisted_health(&promoted_digest);
                    return Ok(true);
                }
                Err(error) => {
                    // The candidate loaded and was invoked; under the single-strike H03 policy an
                    // evaluation failure is treated as an artifact failure. Quarantine it and drop
                    // any retained copy that would simply reproduce the same output; a successful
                    // canary never reaches this branch.
                    self.quarantine_digest(&candidate.digest);
                    self.previous.retain(|m| m.digest != candidate.digest);
                    self.rejection = Some(format!(
                        "candidate rejected; active module retained: {error:#}"
                    ));
                    self.rejection_reason = Some(error.reason());
                    // Continue through the old processor in this same frame, avoiding a one-frame
                    // return to the unpatched pose when a replacement fails validation.
                }
            }
        }
        if let Some(module) = self.active.take() {
            // Only the active module is timed. The candidate canary above and the predecessor
            // restore below are activation probes, not steady-state frame evaluations.
            let max_before = self.health.max_eval_micros;
            let total_before = self.health.total_frame_failures;
            let started = std::time::Instant::now();
            let result = module.evaluate(bones, rigs, seconds, output);
            self.health.record_eval(started.elapsed());
            match result {
                Ok(()) => {
                    self.health.record_success();
                    // A success does not change the cumulative total; it only persists when the
                    // timed evaluation raised the per-revision high-water mark.
                    if self.health.max_eval_micros != max_before {
                        self.sync_persisted_health(&module.digest, &module.revision);
                    }
                    self.active = Some(module);
                    return Ok(true);
                }
                Err(error) => {
                    self.health.record_failure();
                    // An active steady-state failure is the only path that increments the
                    // per-revision cumulative total; persist it before a possible demotion. A
                    // saturated total with an unchanged high-water mark has no persisted change.
                    if self.health.total_frame_failures != total_before
                        || self.health.max_eval_micros != max_before
                    {
                        self.sync_persisted_health(&module.digest, &module.revision);
                    }
                    let failure_reason = error.reason();
                    if !self.health.threshold_reached(self.health_policy) {
                        // The policy tolerates this failure, so the module stays active for a later
                        // frame: keep `active` and `previous` exactly as they were and leave the
                        // digest out of the quarantine store. The failed call may have written
                        // partial or invalid values into the caller's buffer, so clear it and
                        // return `Ok(false)`; the caller renders the baseline pose for this frame
                        // and the module is evaluated again on the next frame. The failure stays
                        // observable through `take_active_failure`, but no `RestoreOutcome` is
                        // pushed because nothing was demoted or restored.
                        let failure = format!(
                            "active module {} frame failed below the health threshold; module retained and baseline pose used for this frame: {error:#}",
                            module.revision
                        );
                        self.active = Some(module);
                        output.clear();
                        self.active_failure = Some(failure);
                        self.active_failure_reason = Some(failure_reason);
                        return Ok(false);
                    }
                    let failed_digest = module.digest_bytes();
                    // The active module was already loaded and verified; a failure that reached a
                    // [`PatchHealthPolicy`] threshold is treated as an artifact failure.
                    // Quarantine it and drop retained copies so neither this frame's restore nor a
                    // later manifest can invoke it again.
                    self.quarantine_digest(&module.digest);
                    self.previous.retain(|m| m.digest != module.digest);
                    self.active_failure = Some(format!(
                        "active module {} failed; restoring a previous module: {error:#}",
                        module.revision
                    ));
                    self.active_failure_reason = Some(failure_reason);
                    // A predecessor failure refines only the `Unavailable` outcome reason. A
                    // successful restore leaves no predecessor reason in a persistent slot, so no
                    // stale typed state survives into a later frame.
                    let mut last_failure_reason = failure_reason;
                    while let Some(previous) = self.previous.pop_back() {
                        // A retained copy of the failed binary would fail identically, and any
                        // other digest already in the quarantine store was proved bad earlier.
                        // Skip both without spending this frame re-evaluating them.
                        if failed_digest.is_some() && previous.digest_bytes() == failed_digest {
                            continue;
                        }
                        if self.is_quarantined(&previous.digest) {
                            continue;
                        }
                        match previous.evaluate(bones, rigs, seconds, output) {
                            Ok(()) => {
                                let restored_digest = previous.digest_bytes();
                                let restored_key = previous.digest.clone();
                                self.active = Some(previous);
                                // The restored predecessor is a new activation: its consecutive
                                // streak and last sample start clean, while its cumulative total
                                // and high-water mark are seeded from the per-revision store.
                                self.seed_persisted_health(&restored_key);
                                self.push_restore(RestoreOutcome::Restored {
                                    failed_digest,
                                    restored_digest,
                                });
                                return Ok(true);
                            }
                            Err(prev_error) => {
                                // The predecessor also produced invalid output. Quarantine it and
                                // drop any deeper retained copy, then keep walking newest-first.
                                last_failure_reason = prev_error.reason();
                                self.quarantine_digest(&previous.digest);
                                self.previous.retain(|m| m.digest != previous.digest);
                                self.active_failure = Some(format!(
                                    "active module {} failed ({error:#}); predecessor {} also failed: {prev_error:#}",
                                    module.revision, previous.revision
                                ));
                            }
                        }
                    }
                    self.active = None;
                    // No predecessor produced valid output. The failed module's counters and
                    // timings stay visible; the snapshot's `revision`/`digest` are `None` until a
                    // new activation resets the record.
                    self.push_restore(RestoreOutcome::Unavailable {
                        failed_digest,
                        reason: last_failure_reason,
                    });
                }
            }
        }
        Ok(false)
    }

    // ------------------------------------------------------------------
    // Movement seam. A separate manifest and slot pair, so staging or unloading a movement
    // processor never disturbs the pose module's digest, latch, retention or rollback.
    // ------------------------------------------------------------------

    /// Stage a `MovementModuleV1` candidate named by `<root>/movement.toml`.
    ///
    /// `Ok(true)` means a new candidate was staged; `Ok(false)` means the manifest is absent or
    /// unchanged (including one already active). A typed `Err` leaves the pose slots untouched and
    /// drops any staged movement candidate. A disabled movement seam refuses with
    /// [`MovementRefusal::SeamDisabled`] before the artifact is read. A deterministic parse,
    /// verification or post-verification load failure latches the manifest text; a
    /// bytes-verified post-verification failure also quarantines its digest, so a changed manifest
    /// that still names the same artifact is refused as [`MovementRefusal::Quarantined`] before any
    /// artifact read. Transient I/O and a held `publish.lock` retry unchanged. The full loader text
    /// is in [`Self::take_movement_rejection`].
    pub fn stage_movement_processor(&mut self) -> std::result::Result<bool, MovementRefusal> {
        let text = match std::fs::read_to_string(self.root.join(MOVEMENT_MANIFEST)) {
            Ok(text) => text,
            Err(error) => {
                self.movement_pending = None;
                self.movement_proposal_ready = false;
                self.movement_observed = None;
                self.set_movement_failure_stamp(None);
                return if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(false)
                } else {
                    Err(self.refuse_movement(
                        MovementRefusal::Publication,
                        format!("movement publication: {error}"),
                    ))
                };
            }
        };
        // ADR-0002 §4.3.5: the capability gate runs on the presence of a complete manifest and
        // before any DLL artifact bytes are read or `Library::new` maps the module. A stage call
        // on a disabled seam drops whatever slot it still holds and latches the manifest text so
        // `collect_retired` still pins its artifact. The first call for a manifest is a typed
        // refusal; unchanged repeat polls are `Ok(false)` so a 1 Hz adapter does not spam errors.
        // No artifact stamp is recorded: the seam state, not the file, is the refusal key.
        if !self.is_seam_enabled(ModuleKind::Movement) {
            let already_refused = self.movement_observed.as_deref() == Some(text.as_str());
            self.movement_observed = Some(text);
            self.set_movement_failure_stamp(None);
            // Drop whatever slot is held through the same path as a caller handback so the live
            // movement health record is reset even if this defensive branch is reached directly.
            self.unload_movement_processor();
            return if already_refused {
                Ok(false)
            } else {
                Err(self.refuse_movement(
                    MovementRefusal::SeamDisabled,
                    "movement seam is disabled".to_owned(),
                ))
            };
        }
        if self.movement_observed.as_ref() == Some(&text) {
            // A terminal movement failure latches the manifest text. If the artifact changed on
            // disk without the text changing, clear the latch and retry once; a missing artifact
            // mismatches too. No stamp keeps the old quiet `Ok(false)`.
            if self
                .movement_failure_stamp
                .as_ref()
                .is_some_and(|stamp| !stamp.matches_current(&self.root))
            {
                self.movement_observed = None;
                self.set_movement_failure_stamp(None);
            } else {
                return Ok(false);
            }
        }
        // Any changed movement manifest invalidates the previous observation and any superseded
        // candidate, exactly as the pose manifest does for its own slot.
        self.movement_pending = None;
        self.movement_proposal_ready = false;
        self.movement_observed = None;
        self.set_movement_failure_stamp(None);
        let publication: Publication = match toml::from_str(&text) {
            Ok(publication) => publication,
            Err(error) => {
                // A malformed movement manifest is deterministic content. Latch it so the 1 Hz
                // stage call does not re-parse and re-report the same bytes; a changed manifest
                // cleared the latch above and is retried. There is no trustworthy publication, so
                // no artifact stamp is recorded; H03 adds movement typed reasons.
                self.set_movement_failure_stamp(None);
                self.movement_observed = Some(text);
                return Err(self.refuse_movement(
                    MovementRefusal::Publication,
                    format!("movement publication: {error}"),
                ));
            }
        };
        if self.is_movement_quarantined(&publication.sha1) {
            // A digest quarantined after a verified load or proposal failure is refused before
            // the active-module shortcut or any artifact read/`Library::new`, even when a changed
            // manifest (`B -> C -> B`) names it again. Latch this text so the unchanged manifest
            // is a quiet `Ok(false)`; a genuinely new digest is still evaluated. The digest is the
            // refusal key, so no artifact stamp is recorded.
            let digest = publication.sha1.clone();
            self.set_movement_failure_stamp(None);
            self.movement_observed = Some(text);
            return Err(self.refuse_movement(
                MovementRefusal::Quarantined,
                format!(
                    "movement module digest {digest} is quarantined after a load or proposal failure"
                ),
            ));
        }
        if self.movement_active.as_ref().is_some_and(|module| {
            !publication.revision.is_empty()
                && module.digest == publication.sha1
                && module.filename == publication.library
        }) {
            self.movement_observed = Some(text);
            return Ok(false);
        }
        // Capture the attempted artifact before `MovementLoaded::verify` reads it, for the same
        // reason as the pose path: a post-read stat could describe a publisher's completed repair
        // and mask the next identical-text retry.
        let attempt_stamp = artifact_stamp(&self.root, &publication.library);
        // Verify first, without any native load, so a verification failure carries no verified digest
        // and can never quarantine a declared-but-unproved digest.
        match MovementLoaded::verify(&self.root, &publication) {
            Err(failure) => {
                if failure.transient {
                    // Leave `movement_observed` unlatched so an incomplete copy or a
                    // publisher-held artifact is retried unchanged; never quarantine it.
                    self.set_movement_failure_stamp(None);
                    return Err(self.refuse_movement(failure.refusal, failure.detail));
                }
                // A verification failure carries `verified_digest: None`, so no quarantine.
                if let Some(digest) = failure.verified_digest.as_deref() {
                    self.quarantine_movement_digest(digest);
                }
                self.set_movement_failure_stamp(attempt_stamp);
                self.movement_observed = Some(text);
                Err(self.refuse_movement(failure.refusal, failure.detail))
            }
            Ok((path, digest)) => {
                // Movement loading-window sentinel: bytes verified, so mark the movement module as
                // loading before the irreversible `MovementLoaded::open`/`Library::new`. A native
                // fault inside the open leaves this marker for the next launch to quarantine.
                // A failure here is best effort and never skips the load. Clear only when this call
                // actually wrote a marker; a failed write may have retained a pending-recovery marker.
                let sentinel_written = match self.write_active_sentinel(
                    SentinelKind::Movement,
                    &publication.revision,
                    &digest,
                    SentinelStage::Loading,
                ) {
                    Ok(()) => true,
                    Err(error) => {
                        self.sentinel_error = Some(format!("active sentinel write: {error:#}"));
                        false
                    }
                };
                let open_result =
                    MovementLoaded::open(&self.root, path, digest.clone(), publication);
                if sentinel_written {
                    if let Err(error) = self.clear_active_sentinel() {
                        self.sentinel_error = Some(format!("active sentinel clear: {error:#}"));
                    }
                }
                match open_result {
                    Ok(pending) => {
                        self.movement_pending = Some(pending);
                        // A newly staged candidate has not proposed yet; only its own accepted proposal
                        // may arm the commit gate.
                        self.movement_proposal_ready = false;
                        // A new staged candidate gets a fresh one-shot canary marker.
                        self.movement_canary_attempted = false;
                        self.movement_observed = Some(text);
                        self.set_movement_failure_stamp(None);
                        self.movement_refusal = None;
                        self.movement_rejection = None;
                        Ok(true)
                    }
                    Err(failure) => {
                        if failure.transient {
                            // Leave `movement_observed` unlatched so an incomplete copy or a
                            // publisher-held artifact is retried unchanged; never quarantine it.
                            self.set_movement_failure_stamp(None);
                            return Err(self.refuse_movement(failure.refusal, failure.detail));
                        }
                        // Only a bytes-verified post-verification failure may quarantine.
                        if let Some(failed_digest) = failure.verified_digest.as_deref() {
                            self.quarantine_movement_digest(failed_digest);
                        }
                        self.set_movement_failure_stamp(attempt_stamp);
                        self.movement_observed = Some(text);
                        Err(self.refuse_movement(failure.refusal, failure.detail))
                    }
                }
            }
        }
    }

    /// Whether a movement processor is staged or active.
    pub fn has_movement_processor(&self) -> bool {
        self.movement_pending.is_some() || self.movement_active.is_some()
    }

    /// Whether a movement processor candidate is currently staged and awaiting its first accepted intent.
    pub fn movement_processor_staged(&self) -> bool {
        self.movement_pending.is_some()
    }

    /// Whether a movement processor has completed its first accepted intent.
    pub fn movement_processor_active(&self) -> bool {
        self.movement_active.is_some()
    }

    pub fn movement_revision(&self) -> Option<&str> {
        self.movement_active
            .as_ref()
            .or(self.movement_pending.as_ref())
            .map(|module| module.revision.as_str())
    }

    /// Verified content identity of the active movement processor.
    pub fn movement_digest(&self) -> Option<&str> {
        self.movement_active
            .as_ref()
            .map(|module| module.digest.as_str())
    }

    /// Read-only health of the active movement processor.
    ///
    /// `revision`/`digest` name the processor active at snapshot time, so both are `None` after a
    /// demotion with no retained predecessor or after an unload; a demotion keeps the last
    /// counters visible, while an unload/seam disable resets them. The counters/timings describe
    /// the active steady-state [`Self::movement_proposal`] calls only: a staged candidate's canary
    /// call is not timed or counted. `last_refusal` carries the most recent module-produced
    /// refusal, including one that was tolerated below a [`MovementHealthPolicy`] threshold, and is
    /// cleared by a successful active proposal, a commit or an unload. `quarantined_digests` is the
    /// current movement quarantine length. This returns the live activation counters; the
    /// persisted per-revision store is written separately by [`Self::persist_state`].
    pub fn movement_health_snapshot(&self) -> MovementHealthSnapshot {
        MovementHealthSnapshot {
            revision: self
                .movement_active
                .as_ref()
                .map(|module| module.revision.clone()),
            digest: self
                .movement_active
                .as_ref()
                .map(|module| module.digest.clone()),
            consecutive_frame_failures: self.movement_health.consecutive_frame_failures,
            total_frame_failures: self.movement_health.total_frame_failures,
            last_eval_micros: self.movement_health.last_eval_micros,
            max_eval_micros: self.movement_health.max_eval_micros,
            quarantined_digests: self.movement_quarantined_digests.len(),
            last_refusal: self.movement_health.last_refusal,
        }
    }

    /// Replace the active-movement demotion thresholds. Affects only failures recorded after this
    /// call; existing counters are kept, so relaxing the policy lets a processor that already
    /// failed remain active until the next failure. The movement policy itself is a live override
    /// and is not serialized in this slice, so a change does not dirty `patch-state.toml`; a later
    /// active evaluation/failure persists the counters under the new policy.
    pub fn set_movement_health_policy(&mut self, policy: MovementHealthPolicy) {
        if self.movement_health_policy != policy {
            self.movement_health_policy = policy;
        }
    }

    /// The demotion thresholds currently in force for active movement-processor failures.
    pub fn movement_health_policy(&self) -> MovementHealthPolicy {
        self.movement_health_policy
    }

    pub fn take_movement_refusal(&mut self) -> Option<MovementRefusal> {
        self.movement_refusal.take()
    }

    /// Free-form loader detail for the last movement refusal. Never a success path.
    pub fn take_movement_rejection(&mut self) -> Option<String> {
        self.movement_rejection.take()
    }

    /// Evaluate the staged or active movement processor once and validate its proposal against
    /// `bounds`. This never promotes: the controller evaluates the same proposal and only then
    /// calls [`Self::commit_movement_processor`].
    ///
    /// The first staged proposal is wrapped in a one-shot `active-sentinel.toml` canary marker
    /// for the duration of its `process` call; later staged proposals and active per-frame calls
    /// are unmarked (the latter remain H03).
    ///
    /// Nothing is written to the caller's proposal when a proposal is refused. A `FrameInput`
    /// refusal hands prediction back for this frame but leaves `movement_pending`/`movement_active`
    /// untouched, so the same processor can serve the next valid frame; `NoModule` is likewise
    /// non-destructive. A module-produced `Status` or output-validation failure is counted against
    /// the active processor's [`MovementHealth`]; below both enabled [`MovementHealthPolicy`]
    /// thresholds the active processor is retained, not quarantined, and the call returns
    /// [`MovementRefusal::Tolerated`] with the precise refusal in the health snapshot. At either
    /// threshold it quarantines the verified digest and drops the failing candidate. A staged
    /// candidate keeps the existing single-strike behavior: it clears `movement_pending` and
    /// retains a healthy active processor unless it has the same digest, and never changes active
    /// health. An active threshold failure clears both slots (this seam keeps no retained
    /// predecessor). `SeamDisabled` evicts both. A valid proposal or commit never quarantines.
    pub fn movement_proposal(
        &mut self,
        frame: &MovementFrame,
        bounds: &MovementBounds,
    ) -> std::result::Result<(MovementProposal, MovementProposalSource), MovementRefusal> {
        // Readiness is per-call: every proposal starts unarmed and only a successful staged
        // proposal below may arm the commit gate. A previous tick cannot leak a promotion right.
        self.movement_proposal_ready = false;
        // A seam disabled after staging must not keep proposing or accept a commit; the setter
        // normally evicts first, but this gate keeps a direct registry read authoritative.
        if !self.is_seam_enabled(ModuleKind::Movement) {
            self.unload_movement_processor();
            return Err(self.refuse_movement(
                MovementRefusal::SeamDisabled,
                "movement seam is disabled".to_owned(),
            ));
        }
        let Some(selection) = self.movement_process() else {
            return Err(MovementRefusal::NoModule);
        };
        let MovementSelection {
            process,
            revision,
            digest,
            staged,
        } = selection;
        if !frame.dt_seconds.is_finite() || frame.dt_seconds <= 0.0 {
            // A caller clock error is a per-frame handback, not a module failure: leave both slots
            // untouched so the same staged/active processor can serve the next valid frame.
            return Err(self.refuse_movement(
                MovementRefusal::FrameInput,
                format!("movement module {revision}: non-finite or non-positive frame step"),
            ));
        }
        if !movement_frame_valid(frame, bounds) {
            return Err(self.refuse_movement(
                MovementRefusal::FrameInput,
                format!("movement module {revision}: invalid frame input or bounds"),
            ));
        }
        let mut output = MovementOutput {
            position: frame.position,
            velocity: frame.velocity,
            facing: frame.facing,
            pitch: frame.pitch,
            move_flags: frame.move_flags,
            anim_override: 0,
            emit_move_opcode: 0,
        };
        // Movement canary-window sentinel (one-shot): the first staged proposal marks the
        // candidate as in-canary immediately before the irreversible `process` call. A native
        // fault during that call leaves this marker for the next launch to quarantine. Active
        // proposals never write a marker, and a second staged proposal does not either. A write
        // or clear failure is best effort and never fails or skips the proposal; a failed write
        // leaves no new marker and retains any pending-recovery marker.
        let sentinel_written = if staged && !self.movement_canary_attempted {
            self.movement_canary_attempted = true;
            match self.write_active_sentinel(
                SentinelKind::Movement,
                &revision,
                &digest,
                SentinelStage::Canary,
            ) {
                Ok(()) => true,
                Err(error) => {
                    self.sentinel_error = Some(format!("active sentinel write: {error:#}"));
                    false
                }
            }
        } else {
            false
        };
        // Only the active processor's steady-state call is timed. A staged candidate's first call
        // is its canary/activation probe, so it must not perturb the active health record.
        let max_before = self.movement_health.max_eval_micros;
        let total_before = self.movement_health.total_frame_failures;
        let started = (!staged).then(std::time::Instant::now);
        let status = unsafe { process(frame, &mut output) };
        if let Some(started) = started {
            self.movement_health.record_eval(started.elapsed());
        }
        // Clear the marker this call wrote immediately after `process` returns, on every regular
        // path (success, non-zero status, or output validation below). Only a process fault, a
        // host panic or a failed clear can leave a new marker behind.
        if sentinel_written {
            if let Err(error) = self.clear_active_sentinel() {
                self.sentinel_error = Some(format!("active sentinel clear: {error:#}"));
            }
        }
        if status != 0 {
            let refusal = MovementRefusal::Status(status);
            let detail = format!("movement module {revision} rejected the frame (status {status})");
            if staged {
                // A staged candidate keeps the original single-strike quarantine/drop behavior;
                // the active health record is untouched.
                self.quarantine_movement_digest(&digest);
                return Err(self.reject_movement_candidate(staged, &digest, refusal, detail));
            }
            return Err(self.record_active_movement_failure(
                &revision,
                &digest,
                refusal,
                detail,
                max_before,
                total_before,
            ));
        }
        match validate_movement_output(frame, &output, bounds) {
            Ok(()) => {
                if staged {
                    // Only a staged candidate can be promoted later; an active processor already
                    // owns the slot, so an active proposal never arms the commit gate.
                    self.movement_proposal_ready = true;
                } else {
                    self.movement_proposal_ready = false;
                    self.movement_health.record_success();
                    // A successful active proposal is the end of the previous tolerated
                    // failure: clear the shared refusal/detail channel as well as the snapshot's
                    // `last_refusal`, so a later `take_movement_refusal` cannot report stale
                    // `Tolerated` state after recovery.
                    self.movement_refusal = None;
                    self.movement_rejection = None;
                    // A success does not change the cumulative total; it only persists when the
                    // timed evaluation raised the per-revision high-water mark.
                    if self.movement_health.max_eval_micros != max_before {
                        self.sync_persisted_movement_health(&digest, &revision);
                    }
                }
                Ok((
                    movement_proposal_of(&output),
                    if staged {
                        MovementProposalSource::Staged
                    } else {
                        MovementProposalSource::Active
                    },
                ))
            }
            Err(refusal) => {
                let detail = format!("movement module {revision} output refused ({refusal:?})");
                if staged {
                    // Staged candidates remain single-strike regardless of the active policy.
                    self.quarantine_movement_digest(&digest);
                    return Err(self.reject_movement_candidate(staged, &digest, refusal, detail));
                }
                Err(self.record_active_movement_failure(
                    &revision,
                    &digest,
                    refusal,
                    detail,
                    max_before,
                    total_before,
                ))
            }
        }
    }

    /// Promote the staged processor after the controller accepts a host-validated staged proposal.
    /// Returns whether a promotion happened; only this makes a candidate active.
    ///
    /// A promotion requires a successful [`Self::movement_proposal`] call from the currently staged
    /// candidate. Staging alone never arms this gate. A `FrameInput` failure retains the candidate
    /// but stays unarmed, so it cannot be promoted until a later valid staged proposal; `NoModule`
    /// leaves the empty selection untouched, and a `Status` or output-validation failure drops the
    /// failing candidate instead. A successful commit restarts the live movement health record and
    /// seeds its cumulative total/high-water mark from the persisted digest entry, so the
    /// consecutive streak, last sample and last refusal start clean.
    pub fn commit_movement_processor(&mut self) -> bool {
        if !self.is_seam_enabled(ModuleKind::Movement) {
            self.unload_movement_processor();
            return false;
        }
        if !self.movement_proposal_ready {
            return false;
        }
        let Some(candidate) = self.movement_pending.take() else {
            self.movement_proposal_ready = false;
            return false;
        };
        let digest = candidate.digest.clone();
        self.movement_active = Some(candidate);
        self.seed_persisted_movement_health(&digest);
        self.movement_proposal_ready = false;
        self.movement_canary_attempted = false;
        self.movement_refusal = None;
        true
    }

    /// Withdraw the current staged proposal after the controller refused the host-validated
    /// intent. Clears the commit readiness but keeps the candidate staged for a later frame.
    /// Returns whether a proposal was armed. The adapter must call this when its intent
    /// evaluation rejects the host proposal; otherwise a later [`Self::commit_movement_processor`]
    /// could promote a proposal the controller refused.
    pub fn withdraw_movement_proposal(&mut self) -> bool {
        std::mem::take(&mut self.movement_proposal_ready)
    }

    /// Drop the staged or active movement processor and hand prediction back to the baseline
    /// controller. Returns whether anything was held; a second call is a no-op, so the handback
    /// happens exactly once. The live movement health record is reset, so the snapshot reports no
    /// identity and zeroed counters; a later commit seeds a fresh record from `patch-state.toml`.
    pub fn unload_movement_processor(&mut self) -> bool {
        let staged = self.movement_pending.take().is_some();
        let active = self.movement_active.take().is_some();
        self.movement_proposal_ready = false;
        self.movement_canary_attempted = false;
        self.movement_health.reset();
        // A handback ends any pending tolerated refusal/detail; the slot is empty and a later
        // `take_movement_refusal` must not report stale state from the removed processor.
        self.movement_refusal = None;
        self.movement_rejection = None;
        staged | active
    }

    fn movement_process(&self) -> Option<MovementSelection> {
        if let Some(pending) = self.movement_pending.as_ref() {
            return Some(MovementSelection {
                process: pending.process,
                revision: pending.revision.clone(),
                digest: pending.digest.clone(),
                staged: true,
            });
        }
        self.movement_active.as_ref().map(|active| MovementSelection {
            process: active.process,
            revision: active.revision.clone(),
            digest: active.digest.clone(),
            staged: false,
        })
    }

    fn refuse_movement(&mut self, refusal: MovementRefusal, detail: String) -> MovementRefusal {
        // Any typed refusal means no proposal from the current candidate was accepted.
        self.movement_proposal_ready = false;
        self.movement_refusal = Some(refusal);
        self.movement_rejection = Some(detail);
        refusal
    }

    /// Record one active-processor failure, persist the updated cumulative total, and either
    /// tolerate it or demote the processor.
    ///
    /// Called only for an active (`!staged`) selection. A below-threshold failure keeps
    /// `movement_active`, leaves the digest out of quarantine, and returns
    /// [`MovementRefusal::Tolerated`] while the precise refusal stays in the health snapshot and
    /// the original detail in `take_movement_rejection`. At either enabled threshold it quarantines
    /// the verified digest and clears the slot through [`Self::reject_movement_candidate`],
    /// returning the original refusal.
    fn record_active_movement_failure(
        &mut self,
        revision: &str,
        digest: &str,
        refusal: MovementRefusal,
        detail: String,
        max_before: u64,
        total_before: u32,
    ) -> MovementRefusal {
        self.movement_health.record_failure(refusal);
        // Persist the cumulative total before a possible demotion. A saturated total with an
        // unchanged high-water mark has no persisted change.
        if self.movement_health.total_frame_failures != total_before
            || self.movement_health.max_eval_micros != max_before
        {
            self.sync_persisted_movement_health(digest, revision);
        }
        if !self.movement_health.threshold_reached(self.movement_health_policy) {
            // Keep the active processor for a later frame and leave the digest out of quarantine.
            // The caller renders baseline movement for this frame; `last_refusal` still records the
            // precise module refusal and the rejection detail stays the original message.
            self.movement_proposal_ready = false;
            self.movement_refusal = Some(MovementRefusal::Tolerated);
            self.movement_rejection = Some(detail);
            return MovementRefusal::Tolerated;
        }
        self.quarantine_movement_digest(digest);
        self.reject_movement_candidate(false, digest, refusal, detail)
    }

    /// Record a module-produced movement failure after its digest was quarantined, and dispose of
    /// the failing candidate. A staged candidate is dropped while a healthy active processor is
    /// retained, unless it carries the same digest: identical content would fail identically. An
    /// active processor failure clears both slots because this seam keeps no retained predecessor.
    /// Caller [`MovementRefusal::FrameInput`] errors never reach this helper; they leave both slots
    /// untouched so the same processor can serve the next valid frame. The active health counters
    /// are not reset here: a demotion keeps them visible beside a `None` snapshot identity, while a
    /// staged failure leaves the active record untouched.
    fn reject_movement_candidate(
        &mut self,
        staged: bool,
        digest: &str,
        refusal: MovementRefusal,
        detail: String,
    ) -> MovementRefusal {
        self.movement_proposal_ready = false;
        // Dropping a staged candidate (or both slots on an active failure) ends any
        // one-shot canary attempt for that candidate.
        self.movement_canary_attempted = false;
        if staged {
            self.movement_pending = None;
            if self
                .movement_active
                .as_ref()
                .is_some_and(|module| module.digest.as_str() == digest)
            {
                self.movement_active = None;
            }
        } else {
            self.movement_pending = None;
            self.movement_active = None;
        }
        self.refuse_movement(refusal, detail)
    }
}

/// Registry holding the active state of hot-pluggable gameplay modules (Movement, Souls Combat,
/// Procedural Space/Spawner, and Story Quests). Each slot is independent so swapping or
/// unloading any module falls back cleanly to the baseline client implementation without
/// restarting or breaking the project.
#[derive(Default)]
pub struct ModularGameplayRegistry {
    pub movement_enabled: bool,
    pub combat_enabled: bool,
    pub space_enabled: bool,
    pub story_enabled: bool,
}

impl ModularGameplayRegistry {
    pub const fn new() -> Self {
        Self {
            movement_enabled: false,
            combat_enabled: false,
            space_enabled: false,
            story_enabled: false,
        }
    }

    /// Enable or disable a specific hot-pluggable gameplay seam at runtime.
    pub fn set_seam_enabled(&mut self, kind: benilla_hotpatch_abi::ModuleKind, enabled: bool) {
        match kind {
            benilla_hotpatch_abi::ModuleKind::Pose => {}
            benilla_hotpatch_abi::ModuleKind::Movement => self.movement_enabled = enabled,
            benilla_hotpatch_abi::ModuleKind::Combat => self.combat_enabled = enabled,
            benilla_hotpatch_abi::ModuleKind::Space => self.space_enabled = enabled,
            benilla_hotpatch_abi::ModuleKind::Story => self.story_enabled = enabled,
        }
    }

    pub fn is_seam_enabled(&self, kind: benilla_hotpatch_abi::ModuleKind) -> bool {
        match kind {
            benilla_hotpatch_abi::ModuleKind::Pose => true,
            benilla_hotpatch_abi::ModuleKind::Movement => self.movement_enabled,
            benilla_hotpatch_abi::ModuleKind::Combat => self.combat_enabled,
            benilla_hotpatch_abi::ModuleKind::Space => self.space_enabled,
            benilla_hotpatch_abi::ModuleKind::Story => self.story_enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_hotpatch_abi::{CombatOutput, ModuleKind, MovementOutput, ProceduralSpawn};
    use sha1::{Digest, Sha1};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn modular_gameplay_seams_toggle_independently() {
        let mut reg = ModularGameplayRegistry::new();
        assert!(!reg.is_seam_enabled(ModuleKind::Movement));
        assert!(!reg.is_seam_enabled(ModuleKind::Combat));
        assert!(!reg.is_seam_enabled(ModuleKind::Space));
        assert!(!reg.is_seam_enabled(ModuleKind::Story));

        reg.set_seam_enabled(ModuleKind::Combat, true);
        reg.set_seam_enabled(ModuleKind::Space, true);
        assert!(reg.is_seam_enabled(ModuleKind::Combat));
        assert!(reg.is_seam_enabled(ModuleKind::Space));
        assert!(!reg.is_seam_enabled(ModuleKind::Movement));

        reg.set_seam_enabled(ModuleKind::Combat, false);
        assert!(!reg.is_seam_enabled(ModuleKind::Combat));
    }

    #[test]
    fn module_abi_outputs_validate_finite_bounds() {
        let mv = MovementOutput {
            position: [1.0, 2.0, 3.0],
            velocity: [0.0, 0.0, 0.0],
            facing: 1.5,
            pitch: 0.0,
            move_flags: 0,
            anim_override: 0,
            emit_move_opcode: 0,
        };
        assert!(mv.valid());

        let combat = CombatOutput {
            next_phase: 4, // DodgeIFrame
            phase_elapsed_sec: 0.12,
            stamina: 85.0,
            poise: 100.0,
            lock_on_target: 0x1234,
            primary_hit_target: 0,
            trigger_spell_id: 0,
            anim_kit_override: 0,
            root_motion_speed: 6.5,
            iframe_active: 1,
            hyperarmor_active: 0,
        };
        assert!(combat.valid());

        let spawn = ProceduralSpawn {
            spawn_uid: 1,
            spawn_kind: 1,
            template_entry: 299,
            display_id: 382,
            position: [-8913.0, -134.0, 81.5],
            orientation: 0.0,
            scale: 1.0,
            faction: 14,
            level: 1,
        };
        assert!(spawn.valid());
    }

    #[test]
    fn poll_retries_unchanged_manifest_after_transient_load_failure() {
        let unique = format!(
            "rustwrath-hotpatch-h00-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");

        let mut host = Host::new(root.clone());
        // No ready.toml yet -> Ok(false).
        assert_eq!(host.poll().expect("missing ready.toml is non-fatal"), false);

        // Write ready.toml before the referenced DLL exists on disk.
        let digest = "0123456789abcdef0123456789abcdef01234567";
        let dll_name = format!("pose-{digest}.dll");
        let manifest = format!(
            "revision = \"rev-a\"\nlibrary = \"{dll_name}\"\nsha1 = \"{digest}\"\n"
        );
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");

        // First poll fails because the DLL is not yet present; observed must NOT be latched.
        let first_err = host.poll().expect_err("missing DLL should fail poll");
        assert!(
            transient_io(&first_err),
            "a missing DLL is a transient filesystem failure: {first_err:#}"
        );
        assert!(
            host.observed.is_none(),
            "transient failure must not latch observed manifest text: {first_err:#}"
        );

        // Second poll with identical ready.toml must retry (not return Ok(false)).
        // Write the DLL file with placeholder bytes so we prove the retry advances to SHA-1 check.
        std::fs::write(root.join(&dll_name), b"not-matching-sha1").expect("write dll");
        let second_err = host
            .poll()
            .expect_err("retry should reach digest verification");
        assert!(
            second_err
                .to_string()
                .contains("module digest differs from publication"),
            "second poll should retry and reach digest check, got: {second_err:#}"
        );

        // A digest mismatch is deterministic content: the manifest is now latched and the error is
        // reported once instead of re-reading and re-loading the same artifact at 1 Hz.
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        // The unchanged deterministic manifest is a quiet poll with no new error.
        assert!(!host
            .poll()
            .expect("latched manifest is a quiet poll"));

        // A changed manifest clears the latch and is evaluated again.
        let new_digest = "fedcba9876543210fedcba9876543210fedcba98";
        let new_dll = format!("pose-{new_digest}.dll");
        let new_manifest = format!(
            "revision = \"rev-b\"\nlibrary = \"{new_dll}\"\nsha1 = \"{new_digest}\"\n"
        );
        std::fs::write(root.join("ready.toml"), &new_manifest).expect("rewrite ready.toml");
        std::fs::write(root.join(&new_dll), b"still-not-matching-sha1").expect("write new dll");
        let retry_err = host
            .poll()
            .expect_err("changed manifest should retry and fail again");
        assert!(
            retry_err
                .to_string()
                .contains("module digest differs from publication"),
            "changed manifest should be re-evaluated, got: {retry_err:#}"
        );
        assert_eq!(host.observed.as_deref(), Some(new_manifest.as_str()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_manifest_latches_after_one_error_and_retries_when_rewritten() {
        let unique = format!(
            "rustwrath-hotpatch-malformed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());

        let malformed = "revision = \"unterminated\n";
        std::fs::write(root.join("ready.toml"), malformed).expect("write malformed ready.toml");
        let error = host
            .poll()
            .expect_err("malformed TOML is a terminal publication error");
        assert!(!transient_io(&error), "a TOML syntax error is terminal");
        assert_eq!(host.observed.as_deref(), Some(malformed));

        // The unchanged malformed bytes are not re-parsed and do not report again.
        assert!(!host
            .poll()
            .expect("latched malformed manifest is quiet"));

        // Rewriting a valid manifest clears the latch and retries. The referenced DLL is absent,
        // so the retry is the transient, still-retryable `NotFound` path.
        let digest = "0123456789abcdef0123456789abcdef01234567";
        let dll_name = format!("pose-{digest}.dll");
        let manifest = format!(
            "revision = \"rev-a\"\nlibrary = \"{dll_name}\"\nsha1 = \"{digest}\"\n"
        );
        std::fs::write(root.join("ready.toml"), &manifest).expect("write valid ready.toml");
        let retry_error = host
            .poll()
            .expect_err("a valid manifest reaches the missing DLL");
        assert!(
            transient_io(&retry_error),
            "a missing DLL remains retryable: {retry_error:#}"
        );
        assert!(
            host.observed.is_none(),
            "a transient retry must not latch the manifest"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn publish_lock_keeps_a_digest_mismatch_retryable_until_the_publisher_finishes() {
        let unique = format!(
            "rustwrath-hotpatch-publish-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let digest = "0123456789abcdef0123456789abcdef01234567";
        let dll = format!("pose-{digest}.dll");
        let manifest =
            format!("revision = \"rev-a\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        std::fs::write(root.join(&dll), b"not-matching-sha1").expect("write wrong bytes");
        std::fs::write(root.join("publish.lock"), b"").expect("write lock");

        let mut host = Host::new(root.clone());
        let first = host.poll().expect_err("mismatch under the lock is retried");
        assert!(transient_io(&first) || publish_in_progress(&root));
        assert!(
            host.observed.is_none(),
            "a publisher-held incomplete copy must not latch"
        );
        assert!(
            !host.is_quarantined(digest),
            "a publisher-held incomplete copy must not quarantine"
        );

        std::fs::remove_file(root.join("publish.lock")).expect("remove lock");
        let second = host
            .poll()
            .expect_err("the completed bad artifact reaches verification");
        assert!(second.to_string().contains("module digest differs"));
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert!(!host.poll().expect("latched mismatch stays quiet"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn transient_io_classifies_only_filesystem_availability() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Interrupted,
            std::io::ErrorKind::WouldBlock,
            std::io::ErrorKind::TimedOut,
        ] {
            let error = anyhow::Error::new(std::io::Error::from(kind)).context("wrapped read");
            assert!(transient_io(&error), "{kind:?} should remain retryable");
        }

        // Windows reports ERROR_SHARING_VIOLATION(32)/ERROR_LOCK_VIOLATION(33) as
        // `Uncategorized`, so the raw code must keep a publisher-held artifact retryable.
        #[cfg(windows)]
        for code in [32, 33] {
            let error = anyhow::Error::new(std::io::Error::from_raw_os_error(code));
            assert!(
                transient_io(&error),
                "Windows sharing/lock violation {code} should remain retryable"
            );
        }

        let plain = anyhow::anyhow!("module digest differs from publication");
        assert!(!transient_io(&plain), "a plain content error is terminal");
        let toml_error: anyhow::Error =
            toml::from_str::<Publication>("revision = \"unterminated")
                .err()
                .expect("invalid TOML")
                .into();
        assert!(!transient_io(&toml_error), "a TOML error is terminal");
        let other = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::InvalidData));
        assert!(!transient_io(&other), "non-availability I/O is terminal");
    }

    #[test]
    fn quarantined_digest_is_refused_before_loaded_open_across_manifest_rewrites() {
        let unique = format!(
            "rustwrath-hotpatch-quarantine-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());

        // The bytes are not a valid PE, but their SHA-1 and the immutable filename match the
        // manifest, so verification passes and `Library::new` is the terminal failure. This is the
        // only case where the declared digest is proven and may be quarantined.
        let bytes_b = b"not a pe image B";
        let bytes_c = b"not a pe image C";
        let digest_b = format!("{:x}", Sha1::digest(bytes_b));
        let digest_c = format!("{:x}", Sha1::digest(bytes_c));
        let dll_b = format!("pose-{digest_b}.dll");
        let dll_c = format!("pose-{digest_c}.dll");
        let manifest_b =
            format!("revision = \"rev-b\"\nlibrary = \"{dll_b}\"\nsha1 = \"{digest_b}\"\n");
        let manifest_c =
            format!("revision = \"rev-c\"\nlibrary = \"{dll_c}\"\nsha1 = \"{digest_c}\"\n");

        // B fails after verification (`Library::new` on non-PE bytes) and is quarantined.
        std::fs::write(root.join("ready.toml"), &manifest_b).expect("write B manifest");
        std::fs::write(root.join(&dll_b), bytes_b).expect("write B dll");
        let error_b = host.poll().expect_err("B's post-verification load is terminal");
        assert!(error_b.to_string().contains("loading presentation module"));
        assert!(host.is_quarantined(&digest_b), "B is quarantined");
        assert_eq!(host.observed.as_deref(), Some(manifest_b.as_str()));

        // C, also a verified digest with an invalid image, is quarantined independently.
        std::fs::write(root.join("ready.toml"), &manifest_c).expect("write C manifest");
        std::fs::write(root.join(&dll_c), bytes_c).expect("write C dll");
        let error_c = host.poll().expect_err("C's post-verification load is terminal");
        assert!(error_c.to_string().contains("loading presentation module"));
        assert!(host.is_quarantined(&digest_c), "C is quarantined");
        assert_eq!(host.observed.as_deref(), Some(manifest_c.as_str()));

        // Republish B with its DLL deleted. The quarantine gate must fire before `Loaded::open`,
        // so this is not the transient `NotFound` path even though the file is absent.
        std::fs::remove_file(root.join(&dll_b)).expect("delete B dll");
        std::fs::write(root.join("ready.toml"), &manifest_b).expect("republish B manifest");
        let quarantined = host
            .poll()
            .expect_err("a quarantined digest is refused before loading");
        assert!(
            quarantined.to_string().contains("quarantined"),
            "expected a quarantine refusal, got: {quarantined:#}"
        );
        assert!(
            quarantined.to_string().contains(digest_b.as_str()),
            "the error should name the quarantined digest: {quarantined:#}"
        );
        assert!(
            !transient_io(&quarantined),
            "quarantine refusal must not look like a missing-file retry: {quarantined:#}"
        );
        assert!(
            host.take_rejection().is_none(),
            "the quarantine path must not write the shared candidate-rejection channel"
        );
        assert_eq!(host.observed.as_deref(), Some(manifest_b.as_str()));

        // The unchanged republished text is a quiet poll; a new digest is still evaluated.
        assert!(!host
            .poll()
            .expect("latched quarantine text is a quiet poll"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn digest_mismatch_latches_without_quarantining_the_declared_digest() {
        let unique = format!(
            "rustwrath-hotpatch-no-quarantine-mismatch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());

        let declared = "1111111111111111111111111111111111111111";
        let dll_name = format!("pose-{declared}.dll");
        let manifest =
            format!("revision = \"rev-b\"\nlibrary = \"{dll_name}\"\nsha1 = \"{declared}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        std::fs::write(root.join(&dll_name), b"bytes whose digest is not declared")
            .expect("write wrong bytes");

        let error = host.poll().expect_err("the digest mismatch is terminal");
        assert!(error
            .to_string()
            .contains("module digest differs from publication"));
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert!(
            !host.is_quarantined(declared),
            "an unverified declared digest must not be quarantined"
        );
        assert!(host.quarantined_digests.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wrong_immutable_filename_latches_without_quarantining_the_verified_digest() {
        let unique = format!(
            "rustwrath-hotpatch-no-quarantine-filename-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());

        // The bytes match the declared digest, but the manifest names a different immutable file.
        // That is a manifest/name failure, not proof the content is bad, so it must latch without
        // quarantining the digest.
        let bytes = b"bytes that hash to the declared digest";
        let digest = format!("{:x}", Sha1::digest(bytes));
        let wrong_name = format!("pose-{}.dll", "f".repeat(40));
        let manifest =
            format!("revision = \"rev-f\"\nlibrary = \"{wrong_name}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        std::fs::write(root.join(&wrong_name), bytes).expect("write artifact");

        let error = host
            .poll()
            .expect_err("the wrong immutable filename is terminal");
        assert!(error
            .to_string()
            .contains("immutable digest filename"));
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert!(
            !host.is_quarantined(&digest),
            "a filename-only mismatch must not block a correctly named republish of the same bytes"
        );
        assert!(host.quarantined_digests.is_empty());

        // Republishing those same bytes under the correct immutable name passes verification, so
        // the next failure is the post-verification `Library::new` on non-PE bytes, not a
        // quarantine refusal. That proves the earlier filename failure did not poison the digest.
        let correct_name = format!("pose-{digest}.dll");
        let republished =
            format!("revision = \"rev-f2\"\nlibrary = \"{correct_name}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join(&correct_name), bytes).expect("write correctly named artifact");
        std::fs::write(root.join("ready.toml"), &republished).expect("republish ready.toml");
        let error = host
            .poll()
            .expect_err("the correctly named non-PE artifact reaches loading");
        assert!(error.to_string().contains("loading presentation module"));
        assert!(
            !error.to_string().contains("quarantined"),
            "the same digest must not be refused at the gate: {error:#}"
        );
        assert!(
            host.is_quarantined(&digest),
            "the post-verification load failure now quarantines the proved digest"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn publish_lock_keeps_a_post_verification_failure_retryable_until_the_publisher_finishes() {
        let unique = format!(
            "rustwrath-hotpatch-publish-lock-post-verify-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let bytes = b"not a pe image behind the lock";
        let digest = format!("{:x}", Sha1::digest(bytes));
        let dll = format!("pose-{digest}.dll");
        let manifest = format!("revision = \"rev-a\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        std::fs::write(root.join(&dll), bytes).expect("write artifact");
        std::fs::write(root.join("publish.lock"), b"").expect("write lock");

        let mut host = Host::new(root.clone());
        let first = host
            .poll()
            .expect_err("a load failure under the publisher lock is retried");
        assert!(first.to_string().contains("loading presentation module"));
        assert!(!transient_io(&first), "the load error itself is terminal");
        assert!(
            host.observed.is_none(),
            "the publisher-held artifact must not latch"
        );
        assert!(
            !host.is_quarantined(&digest),
            "and it must not quarantine a possibly incomplete copy"
        );

        std::fs::remove_file(root.join("publish.lock")).expect("remove lock");
        let second = host
            .poll()
            .expect_err("the completed bad artifact is terminal");
        assert!(second.to_string().contains("loading presentation module"));
        assert!(
            host.is_quarantined(&digest),
            "after the lock clears the proved digest is quarantined"
        );
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert!(!host
            .poll()
            .expect("the latched post-verification failure stays quiet"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_dll_transient_failure_does_not_quarantine() {
        let unique = format!(
            "rustwrath-hotpatch-no-quarantine-transient-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());

        let digest = "3333333333333333333333333333333333333333";
        let dll_name = format!("pose-{digest}.dll");
        let manifest =
            format!("revision = \"rev-a\"\nlibrary = \"{dll_name}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");

        let error = host.poll().expect_err("the absent DLL is a transient failure");
        assert!(
            transient_io(&error),
            "a missing DLL must stay retryable: {error:#}"
        );
        assert!(
            !host.is_quarantined(digest),
            "a transient failure has no terminal digest to quarantine"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "no quarantine entry on the transient path"
        );
        assert!(
            host.observed.is_none(),
            "the transient path must not latch the manifest"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_manifest_does_not_quarantine_a_digest() {
        let unique = format!(
            "rustwrath-hotpatch-no-quarantine-malformed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());

        let malformed = "revision = \"unterminated\n";
        std::fs::write(root.join("ready.toml"), malformed).expect("write malformed ready.toml");
        let error = host
            .poll()
            .expect_err("malformed TOML is a terminal publication error");
        assert!(!transient_io(&error), "a TOML syntax error is terminal");
        assert_eq!(host.observed.as_deref(), Some(malformed));
        assert!(
            host.quarantined_digests.is_empty(),
            "a malformed manifest has no trustworthy digest to quarantine"
        );

        // A parseable manifest rejected by `deny_unknown_fields` is equally untrustworthy: the
        // plausible-looking digest was never bound to a valid publication, so nothing is
        // quarantined. The absent DLL is irrelevant because parsing fails first.
        let digest = "4444444444444444444444444444444444444444";
        let unknown_field = format!(
            "revision = \"rev-x\"\nlibrary = \"pose-{digest}.dll\"\nsha1 = \"{digest}\"\nextra = true\n"
        );
        std::fs::write(root.join("ready.toml"), &unknown_field)
            .expect("write unknown-field manifest");
        let error = host
            .poll()
            .expect_err("an unknown manifest field is a terminal publication error");
        assert!(!transient_io(&error));
        assert_eq!(host.observed.as_deref(), Some(unknown_field.as_str()));
        assert!(
            host.quarantined_digests.is_empty(),
            "an unknown-field manifest has no trustworthy digest to quarantine"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn quarantine_store_dedupes_and_is_bounded() {
        let mut host = Host::new(std::env::temp_dir());
        host.quarantine_digest("dup");
        host.quarantine_digest("dup");
        assert_eq!(
            host.quarantined_digests.len(),
            1,
            "a repeated digest must not duplicate"
        );
        assert!(host.is_quarantined("dup"));

        for index in 0..MAX_QUARANTINED_DIGESTS {
            host.quarantine_digest(&format!("digest-{index:02}"));
        }
        assert_eq!(
            host.quarantined_digests.len(),
            MAX_QUARANTINED_DIGESTS,
            "the store is capped"
        );
        assert!(
            !host.is_quarantined("dup"),
            "the oldest entry is evicted at the cap"
        );
        assert!(host.is_quarantined("digest-00"));
        assert!(host.is_quarantined(&format!(
            "digest-{:02}",
            MAX_QUARANTINED_DIGESTS - 1
        )));
    }

    #[test]
    fn host_evaluate_rejects_malformed_rig_spans_and_non_finite_bones_before_module_dispatch() {
        let mut host = Host::new(std::env::temp_dir());
        let mut output = Vec::new();
        let valid_bone = BoneLocal {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
            parent: -1,
        };
        let bad_span = RigSpan {
            entity: 0,
            first: 0,
            count: 2,
        };
        let err = host
            .evaluate(&[valid_bone], &[bad_span], 0.0, &mut output)
            .expect_err("out-of-bounds RigSpan must fail host pre-validation");
        assert!(err.to_string().contains("host rig span out of bounds"));

        let nan_bone = BoneLocal {
            translation: [f32::NAN, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
            parent: -1,
        };
        let good_span = RigSpan {
            entity: 0,
            first: 0,
            count: 1,
        };
        let err = host
            .evaluate(&[nan_bone], &[good_span], 0.0, &mut output)
            .expect_err("NaN input bone must fail host pre-validation");
        assert!(err
            .to_string()
            .contains("host input bone contains non-finite"));
    }

    // ------------------------------------------------------------------
    // Pose evaluation quarantine. These focused cases exercise the canary and active-frame
    // failure paths with in-process function pointers; no native library is built or loaded.
    // ------------------------------------------------------------------

    const CANARY_DIGEST: &str = "1111111111111111111111111111111111111111";
    const ACTIVE_DIGEST: &str = "2222222222222222222222222222222222222222";
    const NEXT_DIGEST: &str = "3333333333333333333333333333333333333333";
    const NEWEST_DIGEST: &str = "4444444444444444444444444444444444444444";
    const BAD_PREDECESSOR_DIGEST: &str = "5555555555555555555555555555555555555555";
    const GOOD_PREDECESSOR_DIGEST: &str = "6666666666666666666666666666666666666666";
    const MOVEMENT_CANARY_DIGEST: &str = "7777777777777777777777777777777777777777";

    /// Counts calls through the test-only dispatch probe. Reset by the host-validation test that
    /// uses it; no other test references the counter.
    static POSE_DISPATCHES: AtomicUsize = AtomicUsize::new(0);

    /// Counts calls through the span-tiling probe. Only the tiling test references this counter, so
    /// it cannot race the host-validation probe above.
    static TILING_DISPATCHES: AtomicUsize = AtomicUsize::new(0);

    /// Root observed by the canary-marker probe. Only the marker-ordering test sets it.
    static CANARY_MARKER_ROOT: std::sync::Mutex<Option<std::path::PathBuf>> =
        std::sync::Mutex::new(None);

    /// Whether the canary-marker probe saw a marker matching the pose canary stage, revision and
    /// digest at [`CANARY_MARKER_ROOT`] while the staged module was being called.
    static CANARY_MARKER_SEEN: AtomicUsize = AtomicUsize::new(0);

    /// Root observed by the movement canary-marker probe.
    static MOVEMENT_CANARY_ROOT: std::sync::Mutex<Option<std::path::PathBuf>> =
        std::sync::Mutex::new(None);

    /// Whether the movement canary-marker probe saw a marker matching the movement canary stage,
    /// revision and digest at [`MOVEMENT_CANARY_ROOT`].
    static MOVEMENT_CANARY_SEEN: AtomicUsize = AtomicUsize::new(0);

    /// Number of times the movement canary-marker probe ran.
    static MOVEMENT_CANARY_CALLS: AtomicUsize = AtomicUsize::new(0);

    /// Serializes the tests that share [`MOVEMENT_CANARY_ROOT`]/[`MOVEMENT_CANARY_SEEN`]/
    /// [`MOVEMENT_CANARY_CALLS`]; default test threads could otherwise interleave their probe
    /// observations.
    static MOVEMENT_CANARY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn pose_test_root(tag: &str) -> std::path::PathBuf {
        let unique = format!(
            "rustwrath-hotpatch-pose-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn valid_pose_bone() -> BoneLocal {
        BoneLocal {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
            parent: -1,
        }
    }

    fn valid_pose_span() -> RigSpan {
        RigSpan {
            entity: 0,
            first: 0,
            count: 1,
        }
    }

    fn pose_outcome_digest(digest: &str) -> ModuleDigest {
        digest_bytes(digest).expect("pose test digests are 40 hex digits")
    }

    unsafe extern "C" fn allow_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        // SAFETY: the host owns the frame and the output buffer for the duration of this call.
        let frame = unsafe { &*frame };
        let bones = unsafe { std::slice::from_raw_parts(frame.bones, frame.bone_count as usize) };
        let output = unsafe { std::slice::from_raw_parts_mut(out, count as usize) };
        for (slot, bone) in output.iter_mut().zip(bones) {
            *slot = *bone;
        }
        0
    }

    unsafe extern "C" fn reject_pose(
        _frame: *const PoseFrame,
        _out: *mut BoneLocal,
        _count: u32,
    ) -> i32 {
        7
    }

    unsafe extern "C" fn invalid_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        let status = unsafe { allow_pose(frame, out, count) };
        if !out.is_null() {
            // SAFETY: `out` points at the host's first output slot for this call.
            unsafe { (*out).translation[0] = f32::NAN };
        }
        status
    }

    unsafe extern "C" fn zero_norm_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        let status = unsafe { allow_pose(frame, out, count) };
        if !out.is_null() {
            // SAFETY: `out` points at the host's first output slot for this call.
            unsafe { (*out).rotation = [0.0; 4] };
        }
        status
    }

    unsafe extern "C" fn changed_parent_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        let status = unsafe { allow_pose(frame, out, count) };
        if !out.is_null() {
            // SAFETY: `out` points at the host's first output slot for this call.
            unsafe { (*out).parent = 0 };
        }
        status
    }

    unsafe extern "C" fn invalid_and_reparented_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        let status = unsafe { allow_pose(frame, out, count) };
        if !out.is_null() {
            // SAFETY: `out` points at the host's first output slot for this call.
            unsafe {
                (*out).translation[0] = f32::NAN;
                (*out).parent = 0;
            }
        }
        status
    }

    unsafe extern "C" fn reject_pose_three(
        _frame: *const PoseFrame,
        _out: *mut BoneLocal,
        _count: u32,
    ) -> i32 {
        3
    }

    unsafe extern "C" fn count_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        POSE_DISPATCHES.fetch_add(1, Ordering::SeqCst);
        unsafe { allow_pose(frame, out, count) }
    }

    unsafe extern "C" fn count_tiling_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        TILING_DISPATCHES.fetch_add(1, Ordering::SeqCst);
        unsafe { allow_pose(frame, out, count) }
    }

    #[test]
    fn loaded_evaluate_reports_typed_reasons_with_unchanged_details() {
        let bones = [valid_pose_bone()];
        let rigs = [valid_pose_span()];
        let mut output = Vec::new();

        let failure = Loaded::probe("rev-status", CANARY_DIGEST, reject_pose)
            .evaluate(&bones, &rigs, 0.0, &mut output)
            .expect_err("a non-zero module status is an evaluation failure");
        assert_eq!(failure.reason(), PatchEvaluationReason::Status(7));
        assert_eq!(
            failure.to_string(),
            "module rev-status rejected pose frame (status 7)"
        );
        assert_eq!(
            format!("{failure:#}"),
            failure.to_string(),
            "the alternate formatter adds no source chain to a typed evaluation failure"
        );

        let failure = Loaded::probe("rev-invalid", CANARY_DIGEST, invalid_pose)
            .evaluate(&bones, &rigs, 0.0, &mut output)
            .expect_err("a non-finite output transform is an evaluation failure");
        assert_eq!(failure.reason(), PatchEvaluationReason::InvalidOutput);
        assert_eq!(
            failure.to_string(),
            "module returned invalid transforms or changed skeleton topology"
        );

        let failure = Loaded::probe("rev-zero-norm", CANARY_DIGEST, zero_norm_pose)
            .evaluate(&bones, &rigs, 0.0, &mut output)
            .expect_err("a zero-norm output rotation is an evaluation failure");
        assert_eq!(failure.reason(), PatchEvaluationReason::InvalidOutput);
        assert_eq!(
            failure.to_string(),
            "module returned invalid transforms or changed skeleton topology"
        );

        let failure = Loaded::probe("rev-topology", CANARY_DIGEST, changed_parent_pose)
            .evaluate(&bones, &rigs, 0.0, &mut output)
            .expect_err("a changed output parent is an evaluation failure");
        assert_eq!(failure.reason(), PatchEvaluationReason::TopologyChanged);
        assert_eq!(
            failure.to_string(),
            "module returned invalid transforms or changed skeleton topology"
        );

        let failure = Loaded::probe(
            "rev-invalid-topology",
            CANARY_DIGEST,
            invalid_and_reparented_pose,
        )
        .evaluate(&bones, &rigs, 0.0, &mut output)
        .expect_err("an invalid transform plus a changed parent is an evaluation failure");
        assert_eq!(
            failure.reason(),
            PatchEvaluationReason::InvalidOutput,
            "transform validity is checked first, so it wins over the topology change"
        );
        assert_eq!(
            failure.to_string(),
            "module returned invalid transforms or changed skeleton topology"
        );

        // `FrameTooLarge` is inspection-only: `Host::evaluate` rejects a batch above the u32 ABI
        // counts before dispatch, and a test cannot allocate a slice that large to reach the
        // `Loaded::evaluate` guard directly.
        assert!(Loaded::probe("rev-ok", CANARY_DIGEST, allow_pose)
            .evaluate(&bones, &rigs, 0.0, &mut output)
            .is_ok());
    }

    #[test]
    fn candidate_rejection_quarantines_digest_and_drops_retained_copy() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, allow_pose));
        // A retained copy of the candidate would reproduce the same failure; it must be dropped.
        host.previous
            .push_back(Loaded::probe("rev-candidate-old", CANARY_DIGEST, allow_pose));
        host.pending = Some(Loaded::probe("rev-candidate", CANARY_DIGEST, invalid_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("candidate rejection falls back to the active module"),
            "the retained active module still produces this frame"
        );
        assert_eq!(host.digest(), Some(ACTIVE_DIGEST));
        assert!(host.is_quarantined(CANARY_DIGEST));
        assert!(
            !host.is_quarantined(ACTIVE_DIGEST),
            "the retained active module must not be quarantined"
        );
        assert!(
            !host
                .previous
                .iter()
                .any(|module| module.digest == CANARY_DIGEST),
            "the retained copy of the rejected candidate is gone"
        );
        let (rejection, reason) = host
            .take_rejection_detail()
            .expect("candidate rejection detail");
        assert!(
            rejection.contains("rev-candidate"),
            "the rejection names the candidate: {rejection}"
        );
        assert_eq!(reason, PatchEvaluationReason::InvalidOutput);
        assert!(
            host.take_rejection_detail().is_none(),
            "the detail-taking accessor clears both the string and the typed reason"
        );
        assert!(
            host.take_restore_outcomes().is_empty(),
            "candidate rejection is not an active-frame failure"
        );

        // The string-only accessor must also clear the paired typed reason so a later detail-taking
        // call cannot observe it as stale state.
        host.pending = Some(Loaded::probe("rev-candidate-two", NEXT_DIGEST, reject_pose));
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the retained active module still produces this frame")
        );
        let rejection = host.take_rejection().expect("string-only rejection detail");
        assert!(rejection.contains("rev-candidate-two"), "{rejection}");
        assert!(
            host.take_rejection_detail().is_none(),
            "take_rejection cleared the paired typed reason"
        );
    }

    #[test]
    fn quarantined_canary_digest_is_refused_before_the_next_poll_reads_the_artifact() {
        let root = pose_test_root("canary-quarantine");
        let mut host = Host::new(root.clone());
        host.pending = Some(Loaded::probe("rev-canary", CANARY_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a rejected canary with no active module is not a host error")
        );
        assert!(host.is_quarantined(CANARY_DIGEST));
        let rejection = host.take_rejection().expect("canary rejection detail");
        assert!(rejection.contains("rev-canary"), "{rejection}");

        // Promote a distinct module so the quarantine must survive a successful activation and a
        // later B -> C -> B manifest rewrite.
        host.pending = Some(Loaded::probe("rev-c", NEXT_DIGEST, allow_pose));
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the distinct C module promotes")
        );
        assert_eq!(host.digest(), Some(NEXT_DIGEST));

        // Publish a manifest naming the rejected digest but deliberately leave the DLL absent. A
        // gate that ran after `verify_publication` would report the transient missing-file path.
        let manifest = format!(
            "revision = \"rev-canary\"\nlibrary = \"pose-{CANARY_DIGEST}.dll\"\nsha1 = \"{CANARY_DIGEST}\"\n"
        );
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        let error = host
            .poll()
            .expect_err("the rejected digest must be refused at the quarantine gate");
        assert!(
            error.to_string().contains("quarantined"),
            "expected a quarantine refusal, got: {error:#}"
        );
        assert!(
            error.to_string().contains(CANARY_DIGEST),
            "the refusal names the rejected digest: {error:#}"
        );
        assert!(
            !transient_io(&error),
            "the quarantine gate must run before the missing-file path: {error:#}"
        );
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert_eq!(
            host.digest(),
            Some(NEXT_DIGEST),
            "the refused manifest must not disturb the active C module"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_frame_failure_quarantines_digest_and_reports_unavailable() {
        let root = pose_test_root("frame-quarantine");
        let mut host = Host::new(root.clone());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("an active evaluation failure is recovered in the host")
        );
        assert!(host.active.is_none(), "the failed active module is demoted");
        assert!(host.is_quarantined(ACTIVE_DIGEST));
        assert_eq!(
            host.take_restore_outcomes(),
            vec![RestoreOutcome::Unavailable {
                failed_digest: Some(pose_outcome_digest(ACTIVE_DIGEST)),
                reason: PatchEvaluationReason::Status(7),
            }]
        );
        let failure = host.take_active_failure().expect("active failure detail");
        assert!(failure.contains("rev-active"), "{failure}");

        // The frame-failure digest must also be refused before the artifact read on a later
        // manifest, exactly as the canary path is.
        let manifest = format!(
            "revision = \"rev-active\"\nlibrary = \"pose-{ACTIVE_DIGEST}.dll\"\nsha1 = \"{ACTIVE_DIGEST}\"\n"
        );
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        let error = host
            .poll()
            .expect_err("the failed active digest must be refused at the quarantine gate");
        assert!(
            error.to_string().contains("quarantined"),
            "expected a quarantine refusal, got: {error:#}"
        );
        assert!(
            error.to_string().contains(ACTIVE_DIGEST),
            "the refusal names the failed active digest: {error:#}"
        );
        assert!(
            !transient_io(&error),
            "the quarantine gate must run before the missing-file path: {error:#}"
        );
        assert!(host.active.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_skips_a_quarantined_newest_predecessor_and_restores_the_next() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        host.previous
            .push_back(Loaded::probe("rev-next", NEXT_DIGEST, allow_pose));
        host.previous
            .push_back(Loaded::probe("rev-newest", NEWEST_DIGEST, allow_pose));
        host.quarantine_digest(NEWEST_DIGEST);

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a valid predecessor is restored in the same frame")
        );
        assert_eq!(host.digest(), Some(NEXT_DIGEST));
        assert_eq!(
            host.take_restore_outcomes(),
            vec![RestoreOutcome::Restored {
                failed_digest: Some(pose_outcome_digest(ACTIVE_DIGEST)),
                restored_digest: Some(pose_outcome_digest(NEXT_DIGEST)),
            }]
        );
        assert!(
            !host
                .previous
                .iter()
                .any(|module| module.digest == NEWEST_DIGEST),
            "the skipped quarantined predecessor is dropped"
        );
    }

    #[test]
    fn failing_restore_predecessor_is_quarantined_and_the_next_valid_one_is_restored() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        // Oldest first: a deeper copy of the bad predecessor, then a good one, then the failing
        // newest predecessor that the restore loop pops first.
        host.previous.push_back(Loaded::probe(
            "rev-bad-old",
            BAD_PREDECESSOR_DIGEST,
            reject_pose,
        ));
        host.previous.push_back(Loaded::probe(
            "rev-good",
            GOOD_PREDECESSOR_DIGEST,
            allow_pose,
        ));
        host.previous.push_back(Loaded::probe(
            "rev-bad",
            BAD_PREDECESSOR_DIGEST,
            reject_pose,
        ));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the next valid predecessor is restored in the same frame")
        );
        assert_eq!(host.digest(), Some(GOOD_PREDECESSOR_DIGEST));
        assert!(
            host.is_quarantined(BAD_PREDECESSOR_DIGEST),
            "a predecessor that fails during restore is quarantined"
        );
        assert!(
            !host
                .previous
                .iter()
                .any(|module| module.digest == BAD_PREDECESSOR_DIGEST),
            "deeper retained copies of the failed predecessor are dropped"
        );
        assert_eq!(
            host.take_restore_outcomes(),
            vec![RestoreOutcome::Restored {
                failed_digest: Some(pose_outcome_digest(ACTIVE_DIGEST)),
                restored_digest: Some(pose_outcome_digest(GOOD_PREDECESSOR_DIGEST)),
            }]
        );
    }

    #[test]
    fn successful_canary_promotes_without_quarantining() {
        let mut host = Host::new(std::env::temp_dir());
        host.pending = Some(Loaded::probe("rev-candidate", CANARY_DIGEST, allow_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a valid canary batch promotes")
        );
        assert_eq!(host.digest(), Some(CANARY_DIGEST));
        assert!(
            host.quarantined_digests.is_empty(),
            "a successful canary must not quarantine"
        );
        assert!(host.take_rejection().is_none());
    }

    // ------------------------------------------------------------------
    // In-memory H03 health snapshot. These cases exercise the counter/timing lifecycle through
    // `Host::evaluate` with in-process probes; no native library is built or loaded.
    // ------------------------------------------------------------------

    #[test]
    fn active_frame_failure_records_health_and_quarantine() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        // Seed a 60 s high-water mark. A probe cannot take 60 s, so a lower `last` after the call
        // proves the failure path actually timed this evaluation; a missing `record_eval` would
        // leave `last` at the seeded 60 s.
        host.health
            .record_eval(std::time::Duration::from_secs(60));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("an active evaluation failure is recovered in the host")
        );
        let snapshot = host.health_snapshot();
        // No predecessor was available, so the active slot is empty. The failed module's counters
        // and timings stay visible; `revision`/`digest` follow the empty active slot.
        assert_eq!(snapshot.revision, None);
        assert_eq!(snapshot.digest, None);
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 1);
        assert!(
            snapshot.last_eval_micros < 60_000_000,
            "the failed active evaluation replaced the seeded last sample"
        );
        assert!(
            snapshot.max_eval_micros >= 60_000_000,
            "the seeded high-water mark is preserved"
        );
        assert!(snapshot.max_eval_micros >= snapshot.last_eval_micros);
        assert_eq!(snapshot.quarantined_digests, host.quarantined_digests.len());
        assert_eq!(
            snapshot.quarantined_digests, 1,
            "the failed digest is the only quarantined digest"
        );
        assert!(host.is_quarantined(ACTIVE_DIGEST));
    }

    #[test]
    fn restored_predecessor_resets_health_counters() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        host.previous
            .push_back(Loaded::probe("rev-restored", NEXT_DIGEST, allow_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a valid predecessor is restored in the same frame")
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.revision.as_deref(), Some("rev-restored"));
        assert_eq!(snapshot.digest.as_deref(), Some(NEXT_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(snapshot.total_frame_failures, 0);
        assert_eq!(snapshot.last_eval_micros, 0);
        assert_eq!(snapshot.max_eval_micros, 0);

        // The failed active module's typed reason is available for this frame's restore event and
        // then cleared. A later successful frame must not resurrect it.
        let (failure, reason) = host
            .take_active_failure_detail()
            .expect("the demoted active failure remains observable");
        assert!(failure.contains("rev-active"), "{failure}");
        assert_eq!(reason, PatchEvaluationReason::Status(7));
        assert!(host.take_active_failure_detail().is_none());
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the restored predecessor produces the next frame")
        );
        assert!(
            host.take_active_failure_detail().is_none(),
            "no stale typed reason survives a successful restore"
        );
    }

    #[test]
    fn unavailable_restore_carries_the_failing_predecessor_reason() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        host.previous.push_back(Loaded::probe(
            "rev-predecessor",
            BAD_PREDECESSOR_DIGEST,
            reject_pose_three,
        ));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the host recovers a failed restore")
        );
        assert_eq!(
            host.take_restore_outcomes(),
            vec![RestoreOutcome::Unavailable {
                failed_digest: Some(pose_outcome_digest(ACTIVE_DIGEST)),
                reason: PatchEvaluationReason::Status(3),
            }],
            "the last predecessor's reason wins over the active module's own reason"
        );
        let (_, active_reason) = host
            .take_active_failure_detail()
            .expect("the active failure remains observable");
        assert_eq!(active_reason, PatchEvaluationReason::Status(7));
    }

    #[test]
    fn failing_restore_predecessor_does_not_increment_active_failures() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        host.previous
            .push_back(Loaded::probe("rev-bad", BAD_PREDECESSOR_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the host recovers a failed restore")
        );
        let snapshot = host.health_snapshot();
        // Only the active module's Err increments the counters. The failing predecessor in the
        // restore loop must not add a second failure.
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 1);
        assert!(host.is_quarantined(ACTIVE_DIGEST));
        assert!(host.is_quarantined(BAD_PREDECESSOR_DIGEST));
        assert!(matches!(
            host.take_restore_outcomes().as_slice(),
            [RestoreOutcome::Unavailable { .. }]
        ));
    }

    #[test]
    fn active_success_after_failure_resets_consecutive_and_keeps_total() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, allow_pose));
        // Seed a prior failure for this active module. The default single-strike policy demotes on
        // the first failure; this directly drives the success branch that must end the consecutive
        // streak. A relaxed PatchHealthPolicy can reach the same sequence through `evaluate`.
        host.health.record_failure();
        // A 60 s high-water mark proves the success path re-times the evaluation: a probe cannot
        // take 60 s, so `last` must drop below the seeded sample.
        host.health
            .record_eval(std::time::Duration::from_secs(60));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the active module still produces this frame")
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(
            snapshot.total_frame_failures, 1,
            "a success does not clear the cumulative failure count"
        );
        assert!(
            snapshot.last_eval_micros < 60_000_000,
            "the successful active evaluation replaced the seeded last sample"
        );
        assert!(
            snapshot.max_eval_micros >= 60_000_000,
            "the seeded high-water mark survives a faster success"
        );
        assert!(snapshot.max_eval_micros >= snapshot.last_eval_micros);
    }

    #[test]
    fn candidate_canary_failure_does_not_change_active_health() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, allow_pose));
        // Seed a cumulative count so an erroneous candidate-failure increment is observable after
        // the retained active module succeeds in the same frame.
        host.health.record_failure();
        host.health.record_failure();
        host.health.record_success();
        host.pending = Some(Loaded::probe("rev-candidate", CANARY_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the retained active module still produces this frame")
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.digest.as_deref(), Some(ACTIVE_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(
            snapshot.total_frame_failures, 2,
            "a candidate canary rejection must not increment the active module's failures"
        );
        assert!(
            snapshot.max_eval_micros >= snapshot.last_eval_micros,
            "the retained active fallback keeps a sane timing high-water mark"
        );
        assert!(host.is_quarantined(CANARY_DIGEST));
        assert!(
            host.take_restore_outcomes().is_empty(),
            "a candidate rejection is not an active-frame failure"
        );
    }

    #[test]
    fn candidate_promotion_resets_health_counters() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-old", ACTIVE_DIGEST, allow_pose));
        host.health.record_failure();
        host.health.record_eval(std::time::Duration::from_micros(77));
        host.pending = Some(Loaded::probe("rev-new", CANARY_DIGEST, allow_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a valid canary batch promotes")
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.revision.as_deref(), Some("rev-new"));
        assert_eq!(snapshot.digest.as_deref(), Some(CANARY_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(snapshot.total_frame_failures, 0);
        assert_eq!(snapshot.last_eval_micros, 0);
        assert_eq!(snapshot.max_eval_micros, 0);
    }

    #[test]
    fn health_snapshot_quarantine_count_matches_store() {
        let mut host = Host::new(std::env::temp_dir());
        let initial = host.health_snapshot();
        assert_eq!(initial, PatchHealthSnapshot::default());
        assert_eq!(initial.quarantined_digests, 0);

        // A rejected canary with no active module quarantines one digest and leaves health at zero.
        host.pending = Some(Loaded::probe("rev-canary", CANARY_DIGEST, reject_pose));
        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a rejected canary with no active module is recovered")
        );
        let after_canary = host.health_snapshot();
        assert_eq!(after_canary.quarantined_digests, 1);
        assert_eq!(after_canary.consecutive_frame_failures, 0);
        assert_eq!(after_canary.total_frame_failures, 0);

        // A failing active module adds a second digest.
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("an active evaluation failure is recovered in the host")
        );
        assert_eq!(host.health_snapshot().quarantined_digests, 2);
        assert_eq!(
            host.health_snapshot().quarantined_digests,
            host.quarantined_digests.len()
        );

        // The snapshot reports the bounded store, not an ever-growing count.
        for index in 0..MAX_QUARANTINED_DIGESTS {
            host.quarantine_digest(&format!("snapshot-digest-{index:02}"));
        }
        let bounded = host.health_snapshot();
        assert_eq!(bounded.quarantined_digests, MAX_QUARANTINED_DIGESTS);
        assert_eq!(bounded.quarantined_digests, host.quarantined_digests.len());

        // Re-quarantining the same digest does not grow the store.
        host.quarantine_digest(CANARY_DIGEST);
        assert_eq!(host.health_snapshot().quarantined_digests, 2);
    }

    #[test]
    fn health_unchanged_by_empty_batch_and_host_errors() {
        let mut host = Host::new(std::env::temp_dir());
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("an active evaluation failure is recovered in the host")
        );
        let after_failure = host.health_snapshot();
        assert_eq!(after_failure.consecutive_frame_failures, 1);
        assert_eq!(after_failure.total_frame_failures, 1);

        assert!(
            !host
                .evaluate(&[], &[], 0.0, &mut output)
                .expect("an empty batch is a quiet no-op")
        );
        assert_eq!(host.health_snapshot(), after_failure);

        let bad_span = RigSpan {
            entity: 0,
            first: 0,
            count: 2,
        };
        assert!(host
            .evaluate(&[valid_pose_bone()], &[bad_span], 0.0, &mut output)
            .is_err());
        assert_eq!(host.health_snapshot(), after_failure);

        assert!(host
            .evaluate(
                &[valid_pose_bone()],
                &[valid_pose_span()],
                f64::NAN,
                &mut output,
            )
            .is_err());
        assert_eq!(host.health_snapshot(), after_failure);
    }

    #[test]
    fn health_counters_saturate_and_keep_high_water_mark() {
        let mut health = PatchHealth::default();
        health.record_eval(std::time::Duration::from_secs(u64::MAX));
        assert_eq!(health.last_eval_micros, u64::MAX);
        assert_eq!(health.max_eval_micros, u64::MAX);

        health.record_eval(std::time::Duration::from_micros(5));
        assert_eq!(health.last_eval_micros, 5);
        assert_eq!(
            health.max_eval_micros, u64::MAX,
            "a faster call keeps the high-water mark"
        );

        health.consecutive_frame_failures = u32::MAX;
        health.total_frame_failures = u32::MAX;
        health.record_failure();
        assert_eq!(health.consecutive_frame_failures, u32::MAX);
        assert_eq!(health.total_frame_failures, u32::MAX);
    }

    // ------------------------------------------------------------------
    // In-memory `PatchHealthPolicy` thresholds. Each sequence probe owns a dedicated counter so a
    // parallel test cannot interleave its frames; no native library is built or loaded.
    // ------------------------------------------------------------------

    /// Fail, succeed, fail, fail. Owned by `consecutive_reset_by_success_then_third_failure_demotes`.
    static CONSECUTIVE_POLICY_STEP: AtomicUsize = AtomicUsize::new(0);
    /// Fail, succeed, fail. Owned by `total_threshold_demotes_with_consecutive_disabled`.
    static TOTAL_POLICY_STEP: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn consecutive_policy_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        let step = CONSECUTIVE_POLICY_STEP.fetch_add(1, Ordering::SeqCst);
        unsafe { allow_pose(frame, out, count) };
        if matches!(step, 0 | 2 | 3) {
            7
        } else {
            0
        }
    }

    unsafe extern "C" fn total_policy_pose(
        frame: *const PoseFrame,
        out: *mut BoneLocal,
        count: u32,
    ) -> i32 {
        let step = TOTAL_POLICY_STEP.fetch_add(1, Ordering::SeqCst);
        unsafe { allow_pose(frame, out, count) };
        if matches!(step, 0 | 2) {
            7
        } else {
            0
        }
    }

    #[test]
    fn default_health_policy_demotes_on_the_first_active_failure() {
        let mut host = Host::new(std::env::temp_dir());
        assert_eq!(
            host.health_policy(),
            PatchHealthPolicy {
                max_consecutive_frame_failures: 1,
                max_total_frame_failures: 3,
            },
            "the default policy preserves the original single-strike behavior"
        );
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("an active evaluation failure is recovered in the host")
        );
        assert!(
            host.active.is_none(),
            "the first failure reaches the default consecutive threshold"
        );
        assert!(host.is_quarantined(ACTIVE_DIGEST));
        assert_eq!(
            host.take_restore_outcomes(),
            vec![RestoreOutcome::Unavailable {
                failed_digest: Some(pose_outcome_digest(ACTIVE_DIGEST)),
                reason: PatchEvaluationReason::Status(7),
            }]
        );
        assert!(host
            .take_active_failure()
            .expect("active failure detail")
            .contains("rev-active"));
    }

    #[test]
    fn consecutive_threshold_keeps_the_active_module_for_one_more_frame() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 0,
        });
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        // A failed call may leave invalid values in the caller's buffer; the non-demoting path
        // must clear it so the caller renders the baseline pose for this frame.
        let mut output = vec![valid_pose_bone()];

        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a below-threshold active failure is not a host error")
        );
        assert!(
            host.active.is_some(),
            "the first failure is below the consecutive threshold"
        );
        assert_eq!(host.digest(), Some(ACTIVE_DIGEST));
        assert!(
            !host.is_quarantined(ACTIVE_DIGEST),
            "a tolerated failure must not quarantine the module"
        );
        assert!(output.is_empty(), "the failed frame's output is cleared");
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.revision.as_deref(), Some("rev-active"));
        assert_eq!(snapshot.digest.as_deref(), Some(ACTIVE_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 1);
        let (failure, reason) = host
            .take_active_failure_detail()
            .expect("the tolerated failure is still observable");
        assert!(failure.contains("rev-active"), "{failure}");
        assert_eq!(reason, PatchEvaluationReason::Status(7));
        assert!(
            host.take_active_failure_detail().is_none(),
            "the detail-taking accessor clears both slots"
        );
        assert!(
            host.take_restore_outcomes().is_empty(),
            "no demotion happened, so no restore outcome is reported"
        );

        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the second failure demotes without a host error")
        );
        assert!(
            host.active.is_none(),
            "the second consecutive failure reaches the threshold"
        );
        assert!(host.is_quarantined(ACTIVE_DIGEST));
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 2);
        assert_eq!(snapshot.total_frame_failures, 2);
        assert_eq!(
            host.take_restore_outcomes(),
            vec![RestoreOutcome::Unavailable {
                failed_digest: Some(pose_outcome_digest(ACTIVE_DIGEST)),
                reason: PatchEvaluationReason::Status(7),
            }]
        );
    }

    #[test]
    fn string_only_take_active_failure_clears_the_paired_typed_reason() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 0,
        });
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the no-limit policy tolerates the failure")
        );
        let failure = host
            .take_active_failure()
            .expect("the tolerated failure is observable");
        assert!(failure.contains("rev-active"), "{failure}");
        assert!(
            host.take_active_failure_detail().is_none(),
            "take_active_failure cleared the paired typed reason"
        );
    }

    #[test]
    fn detail_accessors_degrade_a_missing_typed_reason_to_unknown() {
        let mut host = Host::new(std::env::temp_dir());
        host.rejection = Some("candidate detail".to_owned());
        assert_eq!(
            host.take_rejection_detail(),
            Some(("candidate detail".to_owned(), PatchEvaluationReason::Unknown))
        );
        host.active_failure = Some("active detail".to_owned());
        assert_eq!(
            host.take_active_failure_detail(),
            Some(("active detail".to_owned(), PatchEvaluationReason::Unknown))
        );
    }

    #[test]
    fn consecutive_reset_by_success_then_third_failure_demotes() {
        CONSECUTIVE_POLICY_STEP.store(0, Ordering::SeqCst);
        let mut host = Host::new(std::env::temp_dir());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 0,
        });
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, consecutive_policy_pose));
        let mut output = Vec::new();

        // Frame 1: fail 1/1. Frame 2: succeed, ending the consecutive streak but keeping total 1.
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the first failure is below the threshold")
        );
        assert!(host.active.is_some());
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the active module succeeds")
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(snapshot.total_frame_failures, 1);

        // Frame 3: fail 1/2, still below the consecutive threshold. Frame 4: fail 2/2, demotes.
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the failure after a success is below the threshold")
        );
        assert!(host.active.is_some());
        assert!(!host.is_quarantined(ACTIVE_DIGEST));
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the second consecutive failure demotes")
        );
        assert!(host.active.is_none());
        assert!(host.is_quarantined(ACTIVE_DIGEST));
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 2);
        assert_eq!(
            snapshot.total_frame_failures, 3,
            "the success did not clear the cumulative failure count"
        );
        assert_eq!(
            host.take_restore_outcomes(),
            vec![RestoreOutcome::Unavailable {
                failed_digest: Some(pose_outcome_digest(ACTIVE_DIGEST)),
                reason: PatchEvaluationReason::Status(7),
            }]
        );
    }

    #[test]
    fn total_threshold_demotes_with_consecutive_disabled() {
        TOTAL_POLICY_STEP.store(0, Ordering::SeqCst);
        let mut host = Host::new(std::env::temp_dir());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 2,
        });
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, total_policy_pose));
        let mut output = Vec::new();

        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the first failure is below the total threshold")
        );
        assert!(host.active.is_some());
        assert!(!host.is_quarantined(ACTIVE_DIGEST));
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the active module succeeds between failures")
        );

        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the second failure reaches the total threshold")
        );
        assert!(
            host.active.is_none(),
            "the cumulative threshold demotes even after a success reset the streak"
        );
        assert!(host.is_quarantined(ACTIVE_DIGEST));
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.total_frame_failures, 2);
        assert_eq!(snapshot.consecutive_frame_failures, 1);
    }

    #[test]
    fn zero_thresholds_disable_demotion() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 0,
        });
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a no-limit policy still reports a baseline frame")
        );
        assert!(
            host.active.is_some(),
            "0/0 is the explicit no-limit opt-in, not demote-on-first-failure"
        );
        assert_eq!(host.digest(), Some(ACTIVE_DIGEST));
        assert!(
            host.quarantined_digests.is_empty(),
            "a retained module is never quarantined by a tolerated failure"
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 1);
        assert!(host.take_active_failure().is_some());
        assert!(host.take_restore_outcomes().is_empty());
    }

    #[test]
    fn snapshot_reflects_policy_counters_and_active_identity() {
        let mut host = Host::new(std::env::temp_dir());
        let policy = PatchHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 4,
        };
        host.set_health_policy(policy);
        assert_eq!(host.health_policy(), policy);
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the first failure is below both thresholds")
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.revision.as_deref(), Some("rev-active"));
        assert_eq!(snapshot.digest.as_deref(), Some(ACTIVE_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 1);
        assert_eq!(
            snapshot.quarantined_digests, 0,
            "the tolerated failure did not touch the quarantine store"
        );
    }

    #[test]
    fn non_demoting_failure_preserves_retained_history_and_staged_candidate() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 3,
            max_total_frame_failures: 0,
        });
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        host.previous
            .push_back(Loaded::probe("rev-good", GOOD_PREDECESSOR_DIGEST, allow_pose));

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a below-threshold failure keeps the active module")
        );
        assert_eq!(host.digest(), Some(ACTIVE_DIGEST));
        assert!(
            host.previous
                .iter()
                .any(|module| module.digest == GOOD_PREDECESSOR_DIGEST),
            "a tolerated failure must not consume retained rollback history"
        );
        assert!(
            host.pending.is_none(),
            "`evaluate` takes any staged candidate before the active slot, so no pending survives \
             this call to be cleared by the failure path"
        );

        // A candidate staged after the tolerated failure stays staged across a host-invalid frame
        // (which returns before dispatch) and only leaves `pending` when it is evaluated.
        host.pending = Some(Loaded::probe("rev-next", CANARY_DIGEST, allow_pose));
        assert!(host.has_pending());
        assert!(host
            .evaluate(
                &[valid_pose_bone()],
                &[valid_pose_span()],
                f64::NAN,
                &mut output,
            )
            .is_err());
        assert!(host.has_pending(), "a failed frame did not clear the candidate");
        assert!(
            host.previous
                .iter()
                .any(|module| module.digest == GOOD_PREDECESSOR_DIGEST)
        );

        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the staged candidate promotes on the next valid frame")
        );
        assert_eq!(host.digest(), Some(CANARY_DIGEST));
        assert!(
            host.previous
                .iter()
                .any(|module| module.digest == GOOD_PREDECESSOR_DIGEST),
            "promotion appends the displaced active and keeps the older retained predecessor"
        );
    }

    #[test]
    fn host_input_validation_errors_do_not_quarantine_or_dispatch() {
        let mut host = Host::new(std::env::temp_dir());
        host.pending = Some(Loaded::probe("rev-pending", CANARY_DIGEST, count_pose));
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, count_pose));
        POSE_DISPATCHES.store(0, Ordering::SeqCst);

        let mut output = Vec::new();
        assert!(
            !host
                .evaluate(&[], &[], 0.0, &mut output)
                .expect("an empty batch is a quiet no-op")
        );
        assert!(host.has_pending(), "the empty batch keeps the candidate staged");
        assert!(host.quarantined_digests.is_empty());

        let bad_span = RigSpan {
            entity: 0,
            first: 0,
            count: 2,
        };
        let error = host
            .evaluate(&[valid_pose_bone()], &[bad_span], 0.0, &mut output)
            .expect_err("an out-of-bounds rig span is a host error");
        assert!(error.to_string().contains("host rig span out of bounds"));

        // Two bones but only the first is covered: the spans no longer tile the batch.
        let gap_span = RigSpan {
            entity: 0,
            first: 0,
            count: 1,
        };
        let error = host
            .evaluate(
                &[valid_pose_bone(), valid_pose_bone()],
                &[gap_span],
                0.0,
                &mut output,
            )
            .expect_err("a rig span gap is a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the gap refusal names the tiling requirement: {error:#}"
        );
        assert!(
            error.to_string().contains("coverage"),
            "the gap refusal names the uncovered remainder: {error:#}"
        );

        let mut nan_bone = valid_pose_bone();
        nan_bone.translation[0] = f32::NAN;
        let error = host
            .evaluate(&[nan_bone], &[valid_pose_span()], 0.0, &mut output)
            .expect_err("a non-finite input bone is a host error");
        assert!(error
            .to_string()
            .contains("host input bone contains non-finite"));

        let error = host
            .evaluate(
                &[valid_pose_bone()],
                &[valid_pose_span()],
                f64::NAN,
                &mut output,
            )
            .expect_err("a NaN frame time is a host error");
        assert!(error.to_string().contains("pose frame time is not finite"));
        let error = host
            .evaluate(
                &[valid_pose_bone()],
                &[valid_pose_span()],
                f64::INFINITY,
                &mut output,
            )
            .expect_err("an infinite frame time is a host error");
        assert!(error.to_string().contains("pose frame time is not finite"));

        let mut bad_parent = valid_pose_bone();
        bad_parent.parent = 1;
        let error = host
            .evaluate(&[bad_parent], &[valid_pose_span()], 0.0, &mut output)
            .expect_err("an out-of-range positive parent is a host error");
        assert!(error.to_string().contains("parent is out of bounds"));

        // Two one-bone rigs: parent 1 is inside the batch but outside its one-bone rig.
        let rig_root = valid_pose_bone();
        let mut cross_rig_parent = valid_pose_bone();
        cross_rig_parent.parent = 1;
        let spans = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
            RigSpan {
                entity: 1,
                first: 1,
                count: 1,
            },
        ];
        let error = host
            .evaluate(
                &[rig_root, cross_rig_parent],
                &spans,
                0.0,
                &mut output,
            )
            .expect_err("a parent outside its own rig span is a host error");
        assert!(
            error
                .to_string()
                .contains("parent must precede its child"),
            "got: {error:#}"
        );

        let mut self_parent = valid_pose_bone();
        self_parent.parent = 0;
        let error = host
            .evaluate(&[self_parent], &[valid_pose_span()], 0.0, &mut output)
            .expect_err("a self-parent is a host error");
        assert!(
            error
                .to_string()
                .contains("parent must precede its child"),
            "got: {error:#}"
        );

        let mut forward_parent = valid_pose_bone();
        forward_parent.parent = 1;
        let forward_child = valid_pose_bone();
        // Within a two-bone rig, index 0's parent 1 is in-bounds but points forward.
        let forward_spec = RigSpan {
            entity: 0,
            first: 0,
            count: 2,
        };
        let error = host
            .evaluate(
                &[forward_parent, forward_child],
                &[forward_spec],
                0.0,
                &mut output,
            )
            .expect_err("a forward parent is a host error");
        assert!(
            error
                .to_string()
                .contains("parent must precede its child"),
            "got: {error:#}"
        );

        assert!(
            host.quarantined_digests.is_empty(),
            "host/caller errors are not artifact failures"
        );
        assert!(host.has_pending(), "the staged candidate is untouched");
        assert_eq!(
            host.digest(),
            Some(ACTIVE_DIGEST),
            "the active module was not dispatched or demoted"
        );
        assert_eq!(
            POSE_DISPATCHES.load(Ordering::SeqCst),
            0,
            "no probe was invoked before host validation completed"
        );
    }

    #[test]
    fn valid_multi_bone_parent_chain_is_dispatched() {
        let mut host = Host::new(std::env::temp_dir());
        host.pending = Some(Loaded::probe("rev-chain", CANARY_DIGEST, allow_pose));

        let rig_root = valid_pose_bone();
        let child_root = valid_pose_bone();
        let mut child = valid_pose_bone();
        child.parent = 0;
        let spans = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
            RigSpan {
                entity: 1,
                first: 1,
                count: 2,
            },
        ];
        let mut output = Vec::new();
        assert!(
            host.evaluate(&[rig_root, child_root, child], &spans, 0.0, &mut output)
                .expect("valid per-rig parent chains are dispatched"),
            "the valid candidate promotes"
        );
        assert_eq!(host.digest(), Some(CANARY_DIGEST));
        assert!(host.quarantined_digests.is_empty());
    }

    #[test]
    fn host_rig_span_gap_overlap_reordering_and_empty_rigs_are_rejected_before_dispatch() {
        let mut host = Host::new(std::env::temp_dir());
        host.pending = Some(Loaded::probe("rev-pending", CANARY_DIGEST, count_tiling_pose));
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, count_tiling_pose));
        TILING_DISPATCHES.store(0, Ordering::SeqCst);
        let mut output = Vec::new();

        // Gap: two bones, but the single span covers only the first.
        let gap = RigSpan {
            entity: 0,
            first: 0,
            count: 1,
        };
        let error = host
            .evaluate(
                &[valid_pose_bone(), valid_pose_bone()],
                &[gap],
                0.0,
                &mut output,
            )
            .expect_err("a rig span gap is a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the gap refusal names the tiling requirement: {error:#}"
        );
        assert!(
            error.to_string().contains("coverage"),
            "the gap refusal names the uncovered remainder: {error:#}"
        );

        // Mid-batch gap: a check that only compared the last span's end to the batch length would
        // accept this; the per-span `first == covered` check catches the hole.
        let mid_gap = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
            RigSpan {
                entity: 1,
                first: 2,
                count: 1,
            },
        ];
        let error = host
            .evaluate(
                &[valid_pose_bone(), valid_pose_bone(), valid_pose_bone()],
                &mid_gap,
                0.0,
                &mut output,
            )
            .expect_err("a mid-batch rig span gap is a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the mid-gap refusal names the tiling requirement: {error:#}"
        );

        // Misplaced zero-count: a sum-only check would accept these three spans because the counts
        // total two, but the zero-count span starts before the current coverage.
        let misplaced_zero = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
            RigSpan {
                entity: 1,
                first: 0,
                count: 0,
            },
            RigSpan {
                entity: 2,
                first: 1,
                count: 1,
            },
        ];
        let error = host
            .evaluate(
                &[valid_pose_bone(), valid_pose_bone()],
                &misplaced_zero,
                0.0,
                &mut output,
            )
            .expect_err("a misplaced zero-count rig span is a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the misplaced zero-count refusal names the tiling requirement: {error:#}"
        );

        // Overlap: two spans that both start at bone 0.
        let overlap = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
            RigSpan {
                entity: 1,
                first: 0,
                count: 1,
            },
        ];
        let error = host
            .evaluate(
                &[valid_pose_bone(), valid_pose_bone()],
                &overlap,
                0.0,
                &mut output,
            )
            .expect_err("overlapping rig spans are a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the overlap refusal names the tiling requirement: {error:#}"
        );

        // Overlap with a wider first span: 0..2 then 1..1 would double-cover bone 1.
        let overlap_wide = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 2,
            },
            RigSpan {
                entity: 1,
                first: 1,
                count: 1,
            },
        ];
        let error = host
            .evaluate(
                &[valid_pose_bone(), valid_pose_bone()],
                &overlap_wide,
                0.0,
                &mut output,
            )
            .expect_err("a later span starting inside its predecessor is a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the wide-overlap refusal names the tiling requirement: {error:#}"
        );

        // Reordered spans: bone 1 precedes bone 0.
        let reordered = [
            RigSpan {
                entity: 1,
                first: 1,
                count: 1,
            },
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
        ];
        let error = host
            .evaluate(
                &[valid_pose_bone(), valid_pose_bone()],
                &reordered,
                0.0,
                &mut output,
            )
            .expect_err("reordered rig spans are a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the reorder refusal names the tiling requirement: {error:#}"
        );

        // Empty rigs with a non-empty bone batch cover nothing.
        let error = host
            .evaluate(&[valid_pose_bone()], &[], 0.0, &mut output)
            .expect_err("empty rigs with bones are a host error");
        assert!(
            error.to_string().contains("tile the batch in order"),
            "the empty-rig refusal names the tiling requirement: {error:#}"
        );
        assert!(
            error.to_string().contains("coverage"),
            "the empty-rig refusal names the uncovered batch: {error:#}"
        );

        assert!(
            host.quarantined_digests.is_empty(),
            "tiling is host/caller validation and never quarantines a digest"
        );
        assert!(host.has_pending(), "the staged candidate is untouched");
        assert_eq!(
            host.digest(),
            Some(ACTIVE_DIGEST),
            "the active module was not dispatched or demoted"
        );
        assert_eq!(
            TILING_DISPATCHES.load(Ordering::SeqCst),
            0,
            "no probe was invoked before span tiling completed"
        );
    }

    #[test]
    fn zero_count_tail_span_is_accepted_and_promotes() {
        let mut host = Host::new(std::env::temp_dir());
        host.pending = Some(Loaded::probe("rev-tail", CANARY_DIGEST, allow_pose));

        // A zero-count span at the current coverage advances nothing; the batch is still tiled.
        let spans = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
            RigSpan {
                entity: 1,
                first: 1,
                count: 0,
            },
        ];
        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &spans, 0.0, &mut output)
                .expect("a zero-count tail span still tiles the batch"),
            "the valid candidate promotes"
        );
        assert_eq!(host.digest(), Some(CANARY_DIGEST));
        assert!(
            host.quarantined_digests.is_empty(),
            "a valid tiling batch never quarantines"
        );
    }

    #[test]
    fn zero_count_spans_are_accepted_at_leading_and_middle_positions() {
        let bones = [valid_pose_bone(), valid_pose_bone()];
        let leading_spans = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 0,
            },
            RigSpan {
                entity: 1,
                first: 0,
                count: 2,
            },
        ];
        let middle_spans = [
            RigSpan {
                entity: 0,
                first: 0,
                count: 1,
            },
            RigSpan {
                entity: 1,
                first: 1,
                count: 0,
            },
            RigSpan {
                entity: 2,
                first: 1,
                count: 1,
            },
        ];
        let cases: [(&str, &[BoneLocal], &[RigSpan]); 2] = [
            ("leading", &bones, &leading_spans),
            ("middle", &bones, &middle_spans),
        ];

        for (label, bones, spans) in cases {
            let mut host = Host::new(std::env::temp_dir());
            host.pending = Some(Loaded::probe("rev-zero", CANARY_DIGEST, allow_pose));
            let mut output = Vec::new();
            assert!(
                host.evaluate(bones, spans, 0.0, &mut output)
                    .unwrap_or_else(|error| panic!(
                        "{label} zero-count span must start at the current coverage: {error:#}"
                    )),
                "{label} zero-count span still promotes"
            );
            assert_eq!(host.digest(), Some(CANARY_DIGEST), "{label}");
            assert!(
                host.quarantined_digests.is_empty(),
                "{label} zero-count span never quarantines"
            );
        }
    }

    // ------------------------------------------------------------------
    // Movement seam. These focused cases exercise the staging/validation state machine with
    // in-process function pointers; no native library, socket or Bevy world is involved.
    // ------------------------------------------------------------------

    const MOVEMENT_BOUNDS: MovementBounds = MovementBounds {
        max_step: 0.5,
        max_horizontal_speed: 8.0,
        max_vertical_speed: 60.0,
        max_abs_facing: 6.5,
        permitted_flags: 0x3f,
    };

    fn movement_frame() -> MovementFrame {
        MovementFrame {
            dt_seconds: 0.05,
            position: [1.0, 2.0, 3.0],
            velocity: [0.0; 3],
            facing: 0.0,
            pitch: 0.0,
            camera_yaw: 0.0,
            input_buttons: 0,
            move_flags: 0,
            run_speed: 7.0,
            walk_speed: 3.5,
            swim_speed: 4.7,
            ground_height: 2.0,
            ground_normal: [0.0, 1.0, 0.0],
            liquid_height: f32::NEG_INFINITY,
            stamina_norm: 1.0,
        }
    }

    fn mirrored(input: *const MovementFrame) -> MovementOutput {
        let input = unsafe { &*input };
        MovementOutput {
            position: input.position,
            velocity: input.velocity,
            facing: input.facing,
            pitch: input.pitch,
            move_flags: input.move_flags,
            anim_override: 0,
            emit_move_opcode: 0,
        }
    }

    unsafe extern "C" fn propose_step(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let mut output = mirrored(frame);
        output.position[0] += 0.1;
        output.velocity = [0.0, 0.0, 2.0];
        output.facing = 0.05;
        output.move_flags = 1;
        // SAFETY: the host owns `out` for the duration of the call.
        unsafe { *out = output };
        0
    }

    /// Observe the one-shot movement canary marker during the module call. The probe records
    /// whether it saw the movement/canary marker with the candidate identity, then returns a valid
    /// proposal so the host promotes nothing by itself.
    unsafe extern "C" fn observe_movement_canary(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        MOVEMENT_CANARY_CALLS.fetch_add(1, Ordering::SeqCst);
        let valid = MOVEMENT_CANARY_ROOT
            .lock()
            .expect("movement canary root lock")
            .as_ref()
            .is_some_and(|root| {
                std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
                    .ok()
                    .and_then(|text| toml::from_str::<ActiveSentinelFile>(&text).ok())
                    .is_some_and(|file| {
                        file.kind == SentinelKind::Movement
                            && file.stage == SentinelStage::Canary
                            && file.revision == "rev-candidate"
                            && file.sha1 == MOVEMENT_CANARY_DIGEST
                    })
            });
        MOVEMENT_CANARY_SEEN.store(usize::from(valid), Ordering::SeqCst);
        // SAFETY: the host owns the frame and output buffer for this call.
        unsafe { propose_step(frame, out) }
    }

    unsafe extern "C" fn propose_nan(frame: *const MovementFrame, out: *mut MovementOutput) -> i32 {
        let mut output = mirrored(frame);
        output.position[1] = f32::NAN;
        unsafe { *out = output };
        0
    }

    unsafe extern "C" fn propose_teleport(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let mut output = mirrored(frame);
        output.position[0] += 100.0;
        unsafe { *out = output };
        0
    }

    unsafe extern "C" fn propose_speed(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let mut output = mirrored(frame);
        output.velocity = [0.0, 0.0, 400.0];
        unsafe { *out = output };
        0
    }

    unsafe extern "C" fn propose_facing(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let mut output = mirrored(frame);
        output.facing = 7.0;
        unsafe { *out = output };
        0
    }

    unsafe extern "C" fn propose_flag(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let mut output = mirrored(frame);
        // ROOT is server authority, never delegated to the seam.
        output.move_flags = 0x1000;
        unsafe { *out = output };
        0
    }

    unsafe extern "C" fn propose_clear_flags(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let mut output = mirrored(frame);
        output.move_flags = 0;
        unsafe { *out = output };
        0
    }

    unsafe extern "C" fn propose_wire(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let mut output = mirrored(frame);
        output.emit_move_opcode = 6;
        unsafe { *out = output };
        0
    }

    unsafe extern "C" fn reject_frame(
        _frame: *const MovementFrame,
        _out: *mut MovementOutput,
    ) -> i32 {
        7
    }

    fn probe_host(process: ProcessMovement) -> Host {
        // A unique root per probe keeps the one-shot movement canary marker out of the shared
        // system temp directory when focused tests run in parallel.
        let mut host = Host::new(movement_test_root("movement-probe"));
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe("rev-movement", process));
        host
    }

    fn active_movement_host(tag: &str, process: ProcessMovement) -> Host {
        let mut host = Host::new(movement_test_root(tag));
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_active = Some(MovementLoaded::probe("rev-active", process));
        host
    }

    /// Fail, succeed, fail sequence for the movement total-threshold case.
    static MOVEMENT_HEALTH_STEP: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn movement_health_sequence(
        frame: *const MovementFrame,
        out: *mut MovementOutput,
    ) -> i32 {
        let step = MOVEMENT_HEALTH_STEP.fetch_add(1, Ordering::SeqCst);
        let status = unsafe { propose_step(frame, out) };
        if matches!(step, 0 | 2) {
            7
        } else {
            status
        }
    }

    #[test]
    fn movement_candidate_is_staged_but_not_active_until_committed() {
        let mut host = probe_host(propose_step);
        assert!(host.has_movement_processor());
        assert!(
            !host.movement_processor_active(),
            "staging alone must not activate a candidate"
        );
        let (proposal, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("a valid candidate proposal");
        assert_eq!(source, MovementProposalSource::Staged);
        assert!((proposal.position[0] - 1.1).abs() < 1e-5);
        assert_eq!(proposal.move_flags, 1);
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "a valid proposal never quarantines"
        );
        assert!(
            !host.movement_processor_active(),
            "a proposal is not an activation"
        );
        assert!(host.commit_movement_processor());
        assert!(host.movement_processor_active());
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the active processor proposes");
        assert_eq!(source, MovementProposalSource::Active);
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "a valid commit and active proposal never quarantine"
        );
        assert!(!host.commit_movement_processor(), "nothing left to promote");
    }

    #[test]
    fn movement_commit_requires_a_staged_proposal() {
        let mut host = probe_host(propose_step);
        assert!(
            !host.commit_movement_processor(),
            "staging a candidate alone must not promote it"
        );
        assert!(
            host.movement_pending.is_some(),
            "the refused commit keeps the candidate staged"
        );
        assert!(!host.movement_processor_active());

        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert_eq!(source, MovementProposalSource::Staged);
        assert!(
            host.commit_movement_processor(),
            "a host-validated staged proposal promotes the candidate"
        );
        assert!(host.movement_processor_active());
        assert!(host.movement_pending.is_none());
        assert!(
            !host.commit_movement_processor(),
            "a second commit has nothing to promote"
        );
    }

    #[test]
    fn movement_controller_rejection_withdraws_the_commit_gate() {
        let mut host = probe_host(propose_step);
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert!(host.movement_proposal_ready);
        assert!(
            host.withdraw_movement_proposal(),
            "the armed proposal is withdrawn"
        );
        assert!(!host.movement_proposal_ready);
        assert!(
            host.movement_pending.is_some(),
            "the refused candidate stays staged for a later frame"
        );
        assert!(
            !host.commit_movement_processor(),
            "a withdrawn proposal cannot be committed"
        );
        assert!(
            !host.withdraw_movement_proposal(),
            "a second withdrawal is a no-op"
        );

        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the retained candidate proposes again");
        assert!(host.commit_movement_processor());
        assert!(host.movement_processor_active());
    }

    #[test]
    fn movement_changed_manifest_clears_an_armed_commit_gate() {
        let root = movement_test_root("commit-manifest-reset");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe("rev-m", propose_step));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert!(host.movement_proposal_ready);

        std::fs::write(root.join(MOVEMENT_MANIFEST), "revision = \"unterminated\n")
            .expect("write malformed movement manifest");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert!(!host.movement_proposal_ready);
        assert!(host.movement_pending.is_none());
        assert!(!host.commit_movement_processor());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_changed_manifest_arming_the_active_shortcut_clears_the_commit_gate() {
        let root = movement_test_root("commit-active-shortcut");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-active",
            ACTIVE_DIGEST,
            propose_step,
        ));
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-pending",
            NEWEST_DIGEST,
            propose_step,
        ));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert!(host.movement_proposal_ready);

        // A changed manifest that names the active digest takes the quiet active shortcut, which
        // does not call `refuse_movement`; only the changed-manifest clear can disarm this gate.
        let dll = format!("movement-{ACTIVE_DIGEST}.dll");
        let manifest = movement_publication_text("rev-active", &dll, ACTIVE_DIGEST);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write active manifest");
        assert_eq!(
            host.stage_movement_processor(),
            Ok(false),
            "the active shortcut is a quiet stage"
        );
        assert!(!host.movement_proposal_ready);
        assert!(host.movement_pending.is_none());
        assert!(host.movement_active.is_some());
        assert!(!host.commit_movement_processor());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_failed_frame_after_a_staged_proposal_blocks_the_commit() {
        let mut host = probe_host(propose_step);
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert!(host.movement_proposal_ready);

        let mut frame = movement_frame();
        frame.dt_seconds = 0.0;
        assert_eq!(
            host.movement_proposal(&frame, &MOVEMENT_BOUNDS),
            Err(MovementRefusal::FrameInput)
        );
        assert!(
            host.movement_pending.is_some(),
            "the handback policy retains the candidate"
        );
        assert!(
            !host.commit_movement_processor(),
            "a failed frame is not an accepted proposal"
        );
        assert!(host.movement_pending.is_some(), "the candidate is still staged");
        assert!(!host.movement_processor_active(), "nothing was promoted");

        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the retained candidate serves the next valid frame");
        assert!(
            host.commit_movement_processor(),
            "a later valid staged proposal re-arms the gate"
        );
        assert!(host.movement_processor_active());
    }

    #[test]
    fn movement_failed_staged_module_cannot_be_committed() {
        for process in [reject_frame as ProcessMovement, propose_nan as ProcessMovement] {
            let mut host = probe_host(process);
            assert!(
                host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                    .is_err(),
                "the staged module fails Status or output validation"
            );
            assert!(
                !host.movement_proposal_ready,
                "a Status/validation failure clears the commit gate"
            );
            assert!(
                !host.commit_movement_processor(),
                "a Status/validation failure never arms the commit gate"
            );
            assert!(
                !host.has_movement_processor(),
                "the failed candidate is dropped"
            );
            assert!(!host.movement_processor_active());
        }
    }

    #[test]
    fn movement_active_proposal_does_not_arm_the_commit_gate() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_active = Some(MovementLoaded::probe("rev-active", propose_step));
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the active processor proposes");
        assert_eq!(source, MovementProposalSource::Active);
        assert!(
            !host.movement_proposal_ready,
            "an active proposal never arms the commit gate"
        );
        assert!(
            !host.commit_movement_processor(),
            "an active proposal has nothing to promote"
        );
        assert!(host.movement_processor_active());
        assert!(host.movement_pending.is_none());

        // Even if a caller injects a candidate after an active proposal, readiness stays false:
        // only a later successful staged proposal can arm the gate.
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-next",
            &"e".repeat(40),
            propose_step,
        ));
        assert!(
            !host.commit_movement_processor(),
            "the active proposal did not arm a later injected candidate"
        );
        assert!(host.movement_pending.is_some());
    }

    #[test]
    fn movement_restaging_after_commit_requires_a_new_staged_proposal() {
        let root = movement_test_root("commit-restage");
        let active_digest = "0".repeat(40);
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe("rev-movement", propose_step));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the first candidate proposes");
        assert!(host.commit_movement_processor());
        assert!(host.movement_processor_active());
        assert!(
            !host.movement_proposal_ready,
            "a successful commit disarms the gate"
        );

        // The next candidate reaches the slot through a stage call that clears the previous
        // selection (this root has no movement manifest, so the absent publication clears the
        // slot) and is then placed directly because focused tests cannot build a native DLL.
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert!(!host.movement_proposal_ready);
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-next",
            &"d".repeat(40),
            propose_step,
        ));
        assert!(
            !host.commit_movement_processor(),
            "a re-staged candidate must propose before promotion"
        );
        assert!(host.movement_pending.is_some(), "the new candidate stays staged");
        assert_eq!(
            host.movement_digest(),
            Some(active_digest.as_str()),
            "the previously active processor is unchanged"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_stage_path_resets_an_armed_commit_gate() {
        let root = movement_test_root("commit-restage-reset");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe("rev-movement", propose_step));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert!(host.movement_proposal_ready);
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert!(
            !host.movement_proposal_ready,
            "a selected-state change disarms the gate"
        );
        assert!(host.movement_pending.is_none());
        assert!(!host.commit_movement_processor());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_candidate_with_non_finite_output_is_dropped_without_promotion() {
        let mut host = probe_host(propose_nan);
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::NonFinite)
        );
        assert!(!host.has_movement_processor(), "the candidate is dropped");
        assert!(!host.movement_processor_active());
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::NonFinite)
        );
        assert!(
            host.take_movement_rejection().is_some(),
            "loader detail is kept"
        );
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::NoModule)
        );
    }

    #[test]
    fn movement_output_bounds_flags_and_wire_requests_are_refused() {
        for (process, expected) in [
            (
                propose_teleport as ProcessMovement,
                MovementRefusal::Step(100.0),
            ),
            (
                propose_speed as ProcessMovement,
                MovementRefusal::Speed(400.0),
            ),
            (
                propose_facing as ProcessMovement,
                MovementRefusal::Facing(7.0),
            ),
            (
                propose_flag as ProcessMovement,
                MovementRefusal::Flags(0x1000),
            ),
            (propose_wire as ProcessMovement, MovementRefusal::WireWrite),
            (reject_frame as ProcessMovement, MovementRefusal::Status(7)),
        ] {
            let mut host = probe_host(process);
            let error = host
                .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                .expect_err("an out-of-contract proposal is refused");
            match (error, expected) {
                (MovementRefusal::Step(step), MovementRefusal::Step(want)) => {
                    assert!((step - want).abs() < 1e-3, "step {step} vs {want}");
                }
                (MovementRefusal::Speed(speed), MovementRefusal::Speed(want)) => {
                    assert!((speed - want).abs() < 1e-3, "speed {speed} vs {want}");
                }
                (MovementRefusal::Facing(facing), MovementRefusal::Facing(want)) => {
                    assert!((facing - want).abs() < 1e-3, "facing {facing} vs {want}");
                }
                (got, want) => assert_eq!(got, want),
            }
            assert!(
                !host.has_movement_processor(),
                "a refused proposal leaves no module behind: {expected:?}"
            );
        }
    }

    #[test]
    fn movement_status_rejection_quarantines_the_selected_digest_and_refuses_a_rewrite() {
        let root = movement_test_root("status-quarantine");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe("rev-movement", reject_frame));
        let probe_digest = "0".repeat(40);

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(
            host.is_movement_quarantined(&probe_digest),
            "a module-produced status failure quarantines the selected digest"
        );
        assert!(!host.has_movement_processor(), "the refused slot is dropped");
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::Status(7))
        );

        // Republishing the same artifact under changed manifest text must be refused before any
        // artifact read, even though its DLL is absent. That distinguishes the quarantine gate
        // from the transient missing-file path.
        let dll = format!("movement-{probe_digest}.dll");
        let manifest = movement_publication_text("rev-movement", &dll, &probe_digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement manifest");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Quarantined)
        );
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::Quarantined),
            "the quarantine refusal is drained before the quiet poll"
        );
        let detail = host.take_movement_rejection().expect("quarantine detail");
        assert!(detail.contains("quarantined"), "detail: {detail}");
        assert!(
            detail.contains(&probe_digest),
            "detail names the quarantined digest: {detail}"
        );
        assert!(
            !detail.contains("No such file") && !detail.contains("os error 2"),
            "the quarantine gate fires before the missing-file path: {detail}"
        );
        assert!(
            !host.has_movement_processor(),
            "a quarantined digest stages nothing"
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));

        // The unchanged republished text is a quiet poll.
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert_eq!(host.take_movement_refusal(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_output_validation_rejection_quarantines_the_selected_digest() {
        let probe_digest = "0".repeat(40);
        for (process, expected) in [
            (propose_nan as ProcessMovement, MovementRefusal::NonFinite),
            (
                propose_teleport as ProcessMovement,
                MovementRefusal::Step(100.0),
            ),
            (
                propose_speed as ProcessMovement,
                MovementRefusal::Speed(400.0),
            ),
            (
                propose_facing as ProcessMovement,
                MovementRefusal::Facing(7.0),
            ),
            (
                propose_flag as ProcessMovement,
                MovementRefusal::Flags(0x1000),
            ),
            (propose_wire as ProcessMovement, MovementRefusal::WireWrite),
        ] {
            let mut host = probe_host(process);
            let error = host
                .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                .expect_err("an out-of-contract proposal is refused");
            assert_eq!(error, expected);
            assert!(
                host.is_movement_quarantined(&probe_digest),
                "an output-validation failure quarantines the selected digest: {expected:?}"
            );
            assert!(
                !host.has_movement_processor(),
                "a refused proposal leaves no module behind: {expected:?}"
            );
        }
    }

    #[test]
    fn movement_frame_input_errors_do_not_quarantine_the_selected_digest() {
        let probe_digest = "0".repeat(40);
        for dt_seconds in [0.0_f32, f32::NAN] {
            let mut host = probe_host(propose_step);
            let mut frame = movement_frame();
            frame.dt_seconds = dt_seconds;
            assert_eq!(
                host.movement_proposal(&frame, &MOVEMENT_BOUNDS),
                Err(MovementRefusal::FrameInput)
            );
            assert!(
                !host.is_movement_quarantined(&probe_digest),
                "a caller clock error never quarantines the module: dt {dt_seconds}"
            );
            assert!(
                host.has_movement_processor(),
                "a caller clock error retains the staged processor: dt {dt_seconds}"
            );
            assert!(
                host.movement_pending.is_some(),
                "the staged candidate stays selected: dt {dt_seconds}"
            );
            assert_eq!(
                host.take_movement_refusal(),
                Some(MovementRefusal::FrameInput)
            );
            assert!(
                host.take_movement_rejection().is_some(),
                "the caller-error detail is kept"
            );
            let (_, source) = host
                .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                .expect("the retained staged processor serves the next valid frame");
            assert_eq!(source, MovementProposalSource::Staged);
        }

        // Non-finite scalar frame inputs are caller errors too. The `liquid_height`
        // NEG_INFINITY sentinel is the one allowed non-finite value.
        let mut cases: Vec<(&str, MovementFrame)> = Vec::new();
        let mut frame = movement_frame();
        frame.position[0] = f32::NAN;
        cases.push(("position", frame));
        let mut frame = movement_frame();
        frame.velocity[1] = f32::INFINITY;
        cases.push(("velocity", frame));
        let mut frame = movement_frame();
        frame.facing = f32::NAN;
        cases.push(("facing", frame));
        let mut frame = movement_frame();
        frame.pitch = f32::INFINITY;
        cases.push(("pitch", frame));
        let mut frame = movement_frame();
        frame.camera_yaw = f32::NAN;
        cases.push(("camera_yaw", frame));
        let mut frame = movement_frame();
        frame.run_speed = f32::INFINITY;
        cases.push(("run_speed", frame));
        let mut frame = movement_frame();
        frame.walk_speed = f32::NAN;
        cases.push(("walk_speed", frame));
        let mut frame = movement_frame();
        frame.swim_speed = f32::INFINITY;
        cases.push(("swim_speed", frame));
        let mut frame = movement_frame();
        frame.ground_height = f32::NAN;
        cases.push(("ground_height", frame));
        let mut frame = movement_frame();
        frame.ground_normal[2] = f32::INFINITY;
        cases.push(("ground_normal", frame));
        let mut frame = movement_frame();
        frame.liquid_height = f32::NAN;
        cases.push(("liquid_height", frame));
        let mut frame = movement_frame();
        frame.stamina_norm = f32::INFINITY;
        cases.push(("stamina_norm", frame));
        let mut frame = movement_frame();
        frame.facing = 7.0;
        cases.push(("facing outside max_abs_facing", frame));
        let mut frame = movement_frame();
        frame.velocity[0] = 9.0;
        cases.push(("horizontal speed outside max_horizontal_speed", frame));
        let mut frame = movement_frame();
        frame.velocity[1] = 61.0;
        cases.push(("vertical speed outside max_vertical_speed", frame));

        for (field, frame) in cases {
            let mut host = probe_host(propose_step);
            assert_eq!(
                host.movement_proposal(&frame, &MOVEMENT_BOUNDS),
                Err(MovementRefusal::FrameInput),
                "invalid {field} is a caller error"
            );
            assert!(
                !host.is_movement_quarantined(&probe_digest),
                "invalid {field} must not quarantine the processor"
            );
            assert!(
                host.has_movement_processor(),
                "invalid {field} retains the staged processor"
            );
        }

        for (label, bounds) in [
            (
                "negative max_step",
                MovementBounds {
                    max_step: -1.0,
                    ..MOVEMENT_BOUNDS
                },
            ),
            (
                "NaN max_horizontal_speed",
                MovementBounds {
                    max_horizontal_speed: f32::NAN,
                    ..MOVEMENT_BOUNDS
                },
            ),
            (
                "negative max_vertical_speed",
                MovementBounds {
                    max_vertical_speed: -1.0,
                    ..MOVEMENT_BOUNDS
                },
            ),
            (
                "NaN max_abs_facing",
                MovementBounds {
                    max_abs_facing: f32::NAN,
                    ..MOVEMENT_BOUNDS
                },
            ),
        ] {
            let mut host = probe_host(propose_step);
            assert_eq!(
                host.movement_proposal(&movement_frame(), &bounds),
                Err(MovementRefusal::FrameInput),
                "{label} is a caller error"
            );
            assert!(
                !host.is_movement_quarantined(&probe_digest),
                "{label} must not quarantine the processor"
            );
            assert!(
                host.has_movement_processor(),
                "{label} retains the movement slots"
            );
        }

        // Input move_flags may carry root/falling/swimming state that the host does not delegate
        // for output. The module must clear it; the caller is not rejected for carrying it.
        let mut flags_host = probe_host(propose_clear_flags);
        let mut flags_frame = movement_frame();
        flags_frame.move_flags = 0x1000;
        let (proposal, _) = flags_host
            .movement_proposal(&flags_frame, &MOVEMENT_BOUNDS)
            .expect("non-delegated input flags are accepted when the module does not echo them");
        assert_eq!(proposal.move_flags, 0);
        assert!(!flags_host.is_movement_quarantined(&probe_digest));

        // +INFINITY remains the unbounded sentinel for the scalar caps.
        let unbounded = MovementBounds {
            max_step: f32::INFINITY,
            max_horizontal_speed: f32::INFINITY,
            max_vertical_speed: f32::INFINITY,
            max_abs_facing: f32::INFINITY,
            permitted_flags: MOVEMENT_BOUNDS.permitted_flags,
        };
        let mut unbounded_host = probe_host(propose_step);
        assert!(
            unbounded_host
                .movement_proposal(&movement_frame(), &unbounded)
                .is_ok(),
            "the +INFINITY bounds sentinel is accepted"
        );

        let mut sentinel_host = probe_host(propose_step);
        assert!(
            sentinel_host
                .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                .is_ok(),
            "the NEG_INFINITY liquid sentinel is a valid frame input"
        );
        assert!(!sentinel_host.is_movement_quarantined(&probe_digest));
    }

    #[test]
    fn movement_frame_input_retains_a_staged_probe_for_the_next_valid_frame() {
        let mut host = probe_host(propose_step);
        let active_digest = "a".repeat(40);
        host.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-active",
            &active_digest,
            propose_step,
        ));
        let mut frame = movement_frame();
        frame.dt_seconds = 0.0;
        assert_eq!(
            host.movement_proposal(&frame, &MOVEMENT_BOUNDS),
            Err(MovementRefusal::FrameInput)
        );
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "a caller frame error never quarantines either selected digest"
        );
        assert!(host.has_movement_processor());
        assert!(host.movement_pending.is_some());
        assert!(
            host.movement_active.is_some(),
            "the active processor is retained too"
        );
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::FrameInput)
        );
        assert!(
            host.take_movement_rejection().is_some(),
            "the caller-error detail is kept"
        );
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the retained staged processor serves the next valid frame");
        assert_eq!(source, MovementProposalSource::Staged);
        assert!(host.movement_pending.is_some());
        assert!(host.movement_active.is_some());
    }

    #[test]
    fn movement_frame_input_retains_an_active_probe_for_the_next_valid_frame() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_active = Some(MovementLoaded::probe("rev-active", propose_step));
        let mut frame = movement_frame();
        frame.position[0] = f32::NAN;
        assert_eq!(
            host.movement_proposal(&frame, &MOVEMENT_BOUNDS),
            Err(MovementRefusal::FrameInput)
        );
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "a caller frame error never quarantines the active digest"
        );
        assert!(host.movement_active.is_some());
        assert!(host.movement_pending.is_none());
        assert!(host.movement_processor_active());
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::FrameInput)
        );
        assert!(host.take_movement_rejection().is_some());
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the retained active processor serves the next valid frame");
        assert_eq!(source, MovementProposalSource::Active);
    }

    #[test]
    fn staged_movement_failure_retains_a_healthy_active_with_a_different_digest() {
        let mut host = Host::new(movement_test_root("movement-staged-retain"));
        host.set_seam_enabled(ModuleKind::Movement, true);
        let failed_digest = "b".repeat(40);
        let active_digest = "a".repeat(40);
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-staged",
            &failed_digest,
            reject_frame,
        ));
        host.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-active",
            &active_digest,
            propose_step,
        ));

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(
            host.is_movement_quarantined(&failed_digest),
            "the failed staged digest is quarantined"
        );
        assert!(
            !host.is_movement_quarantined(&active_digest),
            "the healthy active digest is not quarantined"
        );
        assert!(
            host.movement_pending.is_none(),
            "the failed staged candidate is dropped"
        );
        assert!(
            host.movement_active.is_some(),
            "the healthy active processor is retained"
        );
        assert!(host.movement_processor_active());
        assert_eq!(host.movement_revision(), Some("rev-active"));
        assert_eq!(host.movement_digest(), Some(active_digest.as_str()));
        assert!(
            !host.commit_movement_processor(),
            "a failed staged candidate leaves nothing to promote"
        );
        assert_eq!(
            host.movement_digest(),
            Some(active_digest.as_str()),
            "the failed commit leaves the retained active unchanged"
        );
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the retained active processor still proposes");
        assert_eq!(source, MovementProposalSource::Active);
    }

    #[test]
    fn staged_movement_failure_without_an_active_leaves_no_processor() {
        let mut host = probe_host(reject_frame);
        let digest = "0".repeat(40);
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(
            host.is_movement_quarantined(&digest),
            "the failed staged digest is quarantined"
        );
        assert!(host.movement_pending.is_none(), "the failed candidate is dropped");
        assert!(host.movement_active.is_none(), "there was no active to retain");
        assert!(!host.has_movement_processor());
    }

    #[test]
    fn staged_movement_failure_with_the_same_active_digest_clears_both_slots() {
        let mut host = Host::new(movement_test_root("movement-staged-same-digest"));
        host.set_seam_enabled(ModuleKind::Movement, true);
        let digest = "c".repeat(40);
        host.movement_pending =
            Some(MovementLoaded::probe_with_digest("rev-staged", &digest, reject_frame));
        host.movement_active =
            Some(MovementLoaded::probe_with_digest("rev-active", &digest, propose_step));

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(host.is_movement_quarantined(&digest));
        assert!(host.movement_pending.is_none());
        assert!(
            host.movement_active.is_none(),
            "identical content would fail identically"
        );
        assert!(host.movement_revision().is_none());
        assert!(host.movement_digest().is_none());
        assert!(!host.has_movement_processor());
    }

    #[test]
    fn active_movement_proposal_failure_quarantines_the_selected_digest() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_active = Some(MovementLoaded::probe("rev-active", reject_frame));
        let probe_digest = "0".repeat(40);
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(
            host.is_movement_quarantined(&probe_digest),
            "an active proposal failure quarantines the selected digest"
        );
        assert!(
            host.movement_active.is_none(),
            "the failed active processor is cleared"
        );
        assert!(host.movement_pending.is_none());
        assert!(host.movement_revision().is_none());
        assert!(host.movement_digest().is_none());
        assert!(!host.has_movement_processor());
    }

    #[test]
    fn movement_no_module_and_disabled_seam_do_not_quarantine() {
        let mut host = Host::new(std::env::temp_dir());
        host.set_seam_enabled(ModuleKind::Movement, true);
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::NoModule)
        );
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "NoModule never quarantines"
        );

        let mut host = probe_host(propose_step);
        host.set_seam_enabled(ModuleKind::Movement, false);
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::SeamDisabled)
        );
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "SeamDisabled never quarantines"
        );
    }

    #[test]
    fn movement_unload_mid_fall_hands_back_exactly_once() {
        let mut host = probe_host(propose_step);
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the candidate proposes");
        assert!(host.commit_movement_processor());
        assert!(host.movement_processor_active());
        assert!(host.unload_movement_processor(), "the handback");
        assert!(!host.movement_processor_active());
        assert!(
            !host.unload_movement_processor(),
            "a second handback would duplicate the fallback"
        );
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::NoModule)
        );
    }

    // ------------------------------------------------------------------
    // Movement health, policy and persistence. These cases use in-process probes through
    // `movement_proposal`; no native movement DLL is built or loaded.
    // ------------------------------------------------------------------

    #[test]
    fn movement_active_success_records_timing_and_resets_consecutive() {
        let mut host = active_movement_host("health-success", propose_step);
        let probe_digest = "0".repeat(40);
        host.movement_health.record_failure(MovementRefusal::Status(7));
        host.movement_health.record_failure(MovementRefusal::Status(7));
        // A 60 s high-water mark proves the success path re-times the call. The in-process probe
        // itself can complete inside the platform timer's resolution (inspection-only edge), so a
        // non-zero `last_eval_micros` is deliberately not asserted.
        host.movement_health
            .record_eval(std::time::Duration::from_secs(60));

        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the active processor proposes");
        assert_eq!(source, MovementProposalSource::Active);
        let snapshot = host.movement_health_snapshot();
        assert_eq!(snapshot.revision.as_deref(), Some("rev-active"));
        assert_eq!(snapshot.digest.as_deref(), Some(probe_digest.as_str()));
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(snapshot.total_frame_failures, 2);
        assert!(
            snapshot.last_eval_micros < 60_000_000,
            "the successful active call replaced the seeded last sample"
        );
        assert!(
            snapshot.max_eval_micros >= 60_000_000,
            "the seeded high-water mark survives a faster success"
        );
        assert_eq!(snapshot.last_refusal, None);
        assert_eq!(snapshot.quarantined_digests, 0);
        assert!(
            host.persisted_movement_health.is_empty(),
            "a success with no new high-water mark does not sync the store"
        );
    }

    #[test]
    fn movement_active_status_failure_below_threshold_is_tolerated() {
        let mut host = active_movement_host("health-tolerated-status", reject_frame);
        let probe_digest = "0".repeat(40);
        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 0,
        });

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        assert!(
            host.movement_processor_active(),
            "a below-threshold failure keeps the processor"
        );
        assert!(
            !host.is_movement_quarantined(&probe_digest),
            "a tolerated failure must not quarantine the digest"
        );
        let snapshot = host.movement_health_snapshot();
        assert_eq!(snapshot.revision.as_deref(), Some("rev-active"));
        assert_eq!(snapshot.digest.as_deref(), Some(probe_digest.as_str()));
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 1);
        assert_eq!(snapshot.last_refusal, Some(MovementRefusal::Status(7)));
        assert_eq!(snapshot.quarantined_digests, 0);
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::Tolerated)
        );
        let detail = host
            .take_movement_rejection()
            .expect("the original failure detail is kept");
        assert!(detail.contains("status 7"), "{detail}");

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7)),
            "the second consecutive failure reaches the enabled threshold"
        );
        assert!(host.movement_active.is_none(), "the threshold demotes");
        assert!(host.is_movement_quarantined(&probe_digest));
        let snapshot = host.movement_health_snapshot();
        assert_eq!(snapshot.revision, None);
        assert_eq!(snapshot.digest, None);
        assert_eq!(snapshot.consecutive_frame_failures, 2);
        assert_eq!(snapshot.total_frame_failures, 2);
        assert_eq!(snapshot.last_refusal, Some(MovementRefusal::Status(7)));
        assert_eq!(snapshot.quarantined_digests, 1);
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::Status(7))
        );
    }

    #[test]
    fn movement_active_output_failure_below_threshold_is_tolerated() {
        let mut host = active_movement_host("health-tolerated-output", propose_nan);
        let probe_digest = "0".repeat(40);
        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 0,
        });

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        assert!(host.movement_processor_active());
        assert!(!host.is_movement_quarantined(&probe_digest));
        let snapshot = host.movement_health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 1);
        assert_eq!(snapshot.last_refusal, Some(MovementRefusal::NonFinite));
        let detail = host
            .take_movement_rejection()
            .expect("the validation detail is kept");
        assert!(detail.contains("NonFinite"), "{detail}");
    }

    #[test]
    fn movement_active_success_and_unload_clear_tolerated_refusal() {
        let mut host = active_movement_host("health-stale-refusal", reject_frame);
        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 0,
        });
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        host.movement_active = Some(MovementLoaded::probe("rev-active", propose_step));
        assert!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                .is_ok(),
            "the active processor recovers"
        );
        assert_eq!(host.take_movement_refusal(), None);
        assert_eq!(host.take_movement_rejection(), None);
        assert_eq!(host.movement_health_snapshot().last_refusal, None);

        host.movement_active = Some(MovementLoaded::probe("rev-active", reject_frame));
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        assert!(host.unload_movement_processor());
        assert_eq!(host.take_movement_refusal(), None);
        assert_eq!(host.take_movement_rejection(), None);
        assert_eq!(
            host.movement_health_snapshot(),
            MovementHealthSnapshot::default()
        );
    }

    #[test]
    fn movement_disabled_stage_resets_the_active_health_record() {
        let root = movement_test_root("health-disabled-stage");
        let digest = "0".repeat(40);
        std::fs::write(
            root.join(MOVEMENT_MANIFEST),
            movement_publication_text(
                "rev-m",
                &format!("movement-{digest}.dll"),
                &digest,
            ),
        )
        .expect("write movement manifest");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_active = Some(MovementLoaded::probe("rev-active", propose_step));
        assert!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                .is_ok()
        );
        host.movement_health.total_frame_failures = 2;
        host.movement_health.consecutive_frame_failures = 1;
        host.seams.movement_enabled = false;
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::SeamDisabled)
        );
        assert!(host.movement_active.is_none());
        assert_eq!(
            host.movement_health_snapshot(),
            MovementHealthSnapshot::default(),
            "the defensive disabled-stage branch resets through unload"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_active_output_failure_at_threshold_quarantines() {
        let mut host = active_movement_host("health-output-threshold", propose_nan);
        let digest = "0".repeat(40);
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::NonFinite),
            "the default single-strike policy demotes on the first active failure"
        );
        assert!(host.movement_active.is_none());
        assert!(host.is_movement_quarantined(&digest));
    }

    #[test]
    fn movement_zero_policy_tolerates_active_failure() {
        let mut host = active_movement_host("health-zero-policy", reject_frame);
        let digest = "0".repeat(40);
        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 0,
        });
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        assert!(host.movement_processor_active());
        assert!(!host.is_movement_quarantined(&digest));
        assert_eq!(
            host.movement_health_snapshot().last_refusal,
            Some(MovementRefusal::Status(7))
        );
    }

    #[test]
    fn movement_total_threshold_demotes_after_a_success() {
        MOVEMENT_HEALTH_STEP.store(0, Ordering::SeqCst);
        let mut host = active_movement_host("health-total", movement_health_sequence);
        let probe_digest = "0".repeat(40);
        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 2,
        });

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        assert!(host.movement_processor_active());
        assert!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
                .is_ok(),
            "the active processor succeeds between failures"
        );
        let snapshot = host.movement_health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(snapshot.total_frame_failures, 1);

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(
            host.movement_active.is_none(),
            "the cumulative threshold demotes even after a success reset the streak"
        );
        assert!(host.is_movement_quarantined(&probe_digest));
        let snapshot = host.movement_health_snapshot();
        assert_eq!(snapshot.consecutive_frame_failures, 1);
        assert_eq!(snapshot.total_frame_failures, 2);
        assert_eq!(snapshot.revision, None);
        assert_eq!(snapshot.last_refusal, Some(MovementRefusal::Status(7)));
    }

    #[test]
    fn staged_movement_failure_keeps_active_health_and_single_strike() {
        let root = movement_test_root("health-staged");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        // A no-limit policy proves a staged canary failure still quarantines and drops.
        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 0,
        });
        let active_digest = "a".repeat(40);
        let failed_digest = "b".repeat(40);
        host.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-active",
            &active_digest,
            propose_step,
        ));
        host.movement_health.record_failure(MovementRefusal::Status(3));
        host.movement_health
            .record_eval(std::time::Duration::from_micros(77));
        host.movement_health.record_failure(MovementRefusal::Status(3));
        let before = host.movement_health_snapshot();
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-staged",
            &failed_digest,
            reject_frame,
        ));

        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(host.is_movement_quarantined(&failed_digest));
        assert!(
            host.movement_pending.is_none(),
            "the staged candidate is dropped"
        );
        assert!(
            host.movement_active.is_some(),
            "the different-digest active is retained"
        );
        let after = host.movement_health_snapshot();
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.digest, before.digest);
        assert_eq!(
            after.consecutive_frame_failures,
            before.consecutive_frame_failures
        );
        assert_eq!(after.total_frame_failures, before.total_frame_failures);
        assert_eq!(after.last_eval_micros, before.last_eval_micros);
        assert_eq!(after.max_eval_micros, before.max_eval_micros);
        assert_eq!(after.last_refusal, before.last_refusal);
        assert_eq!(after.quarantined_digests, before.quarantined_digests + 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_health_lifecycle_ignores_caller_errors_and_resets_on_unload() {
        let mut host = active_movement_host("health-lifecycle", propose_step);
        host.movement_health.record_failure(MovementRefusal::Status(7));
        host.movement_health
            .record_eval(std::time::Duration::from_micros(5));
        let before = host.movement_health_snapshot();

        let mut frame = movement_frame();
        frame.dt_seconds = 0.0;
        assert_eq!(
            host.movement_proposal(&frame, &MOVEMENT_BOUNDS),
            Err(MovementRefusal::FrameInput)
        );
        assert_eq!(host.movement_health_snapshot(), before);

        host.movement_pending = Some(MovementLoaded::probe("rev-staged", propose_step));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert!(host.movement_proposal_ready);
        assert!(host.withdraw_movement_proposal());
        assert_eq!(host.movement_health_snapshot(), before);

        assert!(host.unload_movement_processor());
        assert_eq!(
            host.movement_health_snapshot(),
            MovementHealthSnapshot::default(),
            "unload resets the live record"
        );

        host.movement_active = Some(MovementLoaded::probe("rev-active", propose_step));
        host.movement_health.record_failure(MovementRefusal::Status(9));
        assert_eq!(
            host.movement_health_snapshot().consecutive_frame_failures,
            1
        );
        host.set_seam_enabled(ModuleKind::Movement, false);
        assert_eq!(
            host.movement_health_snapshot(),
            MovementHealthSnapshot::default(),
            "seam disable resets the live record"
        );

        let mut empty = Host::new(movement_test_root("health-nomodule"));
        empty.set_seam_enabled(ModuleKind::Movement, true);
        assert_eq!(
            empty.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::NoModule)
        );
        assert_eq!(
            empty.movement_health_snapshot(),
            MovementHealthSnapshot::default()
        );
    }

    #[test]
    fn movement_commit_seeds_persisted_health_and_later_frames_update_it() {
        let root = movement_test_root("health-commit-seed");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.persisted_movement_health
            .push_back(PersistedRevisionHealth {
                digest: ACTIVE_DIGEST.to_owned(),
                revision: "rev-seeded".to_owned(),
                total_frame_failures: 5,
                max_eval_micros: 123,
            });
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-seeded",
            ACTIVE_DIGEST,
            propose_step,
        ));
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate proposes");
        assert_eq!(source, MovementProposalSource::Staged);
        assert!(host.commit_movement_processor());
        let snapshot = host.movement_health_snapshot();
        assert_eq!(snapshot.revision.as_deref(), Some("rev-seeded"));
        assert_eq!(snapshot.digest.as_deref(), Some(ACTIVE_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(snapshot.total_frame_failures, 5);
        assert_eq!(
            snapshot.last_eval_micros, 0,
            "the staged activation probe is not timed"
        );
        assert_eq!(snapshot.max_eval_micros, 123);
        assert_eq!(snapshot.last_refusal, None);

        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 10,
        });
        host.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-seeded",
            ACTIVE_DIGEST,
            reject_frame,
        ));
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        let failed = host.movement_health_snapshot();
        assert_eq!(failed.consecutive_frame_failures, 1);
        assert_eq!(failed.total_frame_failures, 6);
        assert_eq!(failed.last_refusal, Some(MovementRefusal::Status(7)));
        assert_eq!(host.persisted_movement_health.len(), 1);
        assert_eq!(
            host.persisted_movement_health[0].total_frame_failures, 6,
            "a failure syncs the cumulative total before any demotion"
        );

        host.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-seeded",
            ACTIVE_DIGEST,
            propose_step,
        ));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the same digest proposes");
        let recovered = host.movement_health_snapshot();
        assert_eq!(recovered.consecutive_frame_failures, 0);
        assert_eq!(recovered.total_frame_failures, 6);
        assert_eq!(recovered.last_refusal, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_readmission_after_quarantine_seeds_the_stored_total() {
        let root = movement_test_root("health-readmission");
        let mut first = Host::new(root.clone());
        first.set_seam_enabled(ModuleKind::Movement, true);
        first.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 2,
        });
        first.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-cumulative",
            ACTIVE_DIGEST,
            reject_frame,
        ));
        assert_eq!(
            first.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Tolerated)
        );
        assert_eq!(first.movement_health_snapshot().total_frame_failures, 1);
        assert_eq!(
            first.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::Status(7))
        );
        assert!(first.is_movement_quarantined(ACTIVE_DIGEST));
        assert!(first.persist_state().expect("persist movement health"));

        let mut second = Host::new(root.clone());
        assert_eq!(
            second.load_persistent_state().expect("reload patch state"),
            1
        );
        assert_eq!(second.persisted_movement_health.len(), 1);
        assert_eq!(
            second.persisted_movement_health[0].total_frame_failures, 2
        );
        second.set_seam_enabled(ModuleKind::Movement, true);
        // A normal B -> C -> B rewrite stays refused; explicit re-admission (cap eviction or
        // deleting the quarantine entry) lets the stored total seed the reactivation.
        second.movement_quarantined_digests.clear();
        second.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-cumulative",
            ACTIVE_DIGEST,
            propose_step,
        ));
        second
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the re-admitted candidate proposes");
        assert!(second.commit_movement_processor());
        let snapshot = second.movement_health_snapshot();
        assert_eq!(snapshot.digest.as_deref(), Some(ACTIVE_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(
            snapshot.total_frame_failures, 2,
            "re-admission starts from the stored cumulative total"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_round_trips_movement_health() {
        let root = movement_test_root("state-movement-round-trip");
        let mut host = Host::new(root.clone());
        let policy = MovementHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 7,
        };
        host.set_movement_health_policy(policy);
        assert_eq!(host.movement_health_policy(), policy);
        assert!(
            !host.state_dirty,
            "the movement policy is a live override and is not persisted"
        );
        host.movement_health.total_frame_failures = 3;
        host.movement_health.max_eval_micros = 1234;
        host.sync_persisted_movement_health(ACTIVE_DIGEST, "rev-movement");
        assert!(host.persist_state().expect("persist patch state"));
        let text = std::fs::read_to_string(root.join(PATCH_STATE_FILE))
            .expect("the state file is readable");
        assert!(
            text.contains("[[movement_health.revisions]]"),
            "{text}"
        );
        assert!(text.contains(ACTIVE_DIGEST), "{text}");
        assert!(
            !text.contains('/') && !text.contains(".."),
            "no paths are written: {text}"
        );

        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored.load_persistent_state().expect("load patch state"),
            1
        );
        assert!(restored.persisted_health.is_empty());
        assert_eq!(restored.persisted_movement_health.len(), 1);
        assert_eq!(
            restored.persisted_movement_health[0].digest,
            ACTIVE_DIGEST
        );
        assert_eq!(
            restored.persisted_movement_health[0].revision,
            "rev-movement"
        );
        assert_eq!(
            restored.persisted_movement_health[0].total_frame_failures,
            3
        );
        assert_eq!(restored.persisted_movement_health[0].max_eval_micros, 1234);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_without_movement_health_loads_an_empty_store() {
        let root = movement_test_root("state-no-movement");
        std::fs::write(root.join(PATCH_STATE_FILE), "schema_version = 1\n")
            .expect("write minimal state");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_state()
                .expect("a schema-1 file without the section loads"),
            0
        );
        assert!(host.persisted_movement_health.is_empty());
        assert!(host.persisted_health.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_skips_invalid_movement_health_entries_and_dedupes() {
        let root = movement_test_root("state-movement-invalid");
        let valid = persistent_digest(0x71);
        let other = persistent_digest(0x72);
        let file = PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: None,
            health: Some(PersistedHealth {
                revisions: vec![persisted_revision(ACTIVE_DIGEST, "rev-pose", 1, 1)],
            }),
            movement_health: Some(PersistedHealth {
                revisions: vec![
                    persisted_revision("not-a-digest", "rev-bad", 9, 9),
                    persisted_revision(&other, "", 1, 1),
                    persisted_revision(&valid, "rev-first", 1, 10),
                    persisted_revision(&valid, "rev-updated", 4, 40),
                ],
            }),
            stamps: None,
        };
        std::fs::write(root.join(PATCH_STATE_FILE), patch_state_text(&file))
            .expect("write state");

        let mut host = Host::new(root.clone());
        assert_eq!(host.load_persistent_state().expect("load state"), 2);
        assert_eq!(host.persisted_health.len(), 1);
        assert_eq!(host.persisted_health[0].digest, ACTIVE_DIGEST);
        assert_eq!(host.persisted_movement_health.len(), 1);
        assert_eq!(host.persisted_movement_health[0].digest, valid);
        assert_eq!(
            host.persisted_movement_health[0].revision, "rev-updated",
            "the later duplicate replaces the label and moves to the newest end"
        );
        assert_eq!(
            host.persisted_movement_health[0].total_frame_failures,
            4,
            "the later duplicate's counters win"
        );
        assert_eq!(host.persisted_movement_health[0].max_eval_micros, 40);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_invalid_pose_health_does_not_discard_movement_health() {
        let root = movement_test_root("state-pose-invalid-movement-valid");
        let valid = persistent_digest(0x73);
        let file = PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: None,
            health: Some(PersistedHealth {
                revisions: vec![persisted_revision("not-a-digest", "rev-bad", 1, 1)],
            }),
            movement_health: Some(PersistedHealth {
                revisions: vec![persisted_revision(&valid, "rev-movement", 2, 20)],
            }),
            stamps: None,
        };
        std::fs::write(root.join(PATCH_STATE_FILE), patch_state_text(&file))
            .expect("write state");

        let mut host = Host::new(root.clone());
        assert_eq!(host.load_persistent_state().expect("load state"), 1);
        assert!(
            host.persisted_health.is_empty(),
            "the invalid pose entry is skipped"
        );
        assert_eq!(host.persisted_movement_health.len(), 1);
        assert_eq!(host.persisted_movement_health[0].digest, valid);
        assert_eq!(
            host.persisted_movement_health[0].total_frame_failures,
            2
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_movement_health_load_respects_the_cap() {
        let root = movement_test_root("state-movement-cap");
        let revisions: Vec<PersistedRevisionHealth> = (0..20)
            .map(|index| {
                persisted_revision(
                    &persistent_digest(index),
                    &format!("rev-{index}"),
                    index as u32,
                    index as u64,
                )
            })
            .collect();
        let file = PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: None,
            health: None,
            movement_health: Some(PersistedHealth { revisions }),
            stamps: None,
        };
        std::fs::write(root.join(PATCH_STATE_FILE), patch_state_text(&file))
            .expect("write state");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_state().expect("load state"),
            MAX_PERSISTED_REVISIONS,
            "an oversized list keeps only its newest cap-sized window"
        );
        assert_eq!(
            host.persisted_movement_health.len(),
            MAX_PERSISTED_REVISIONS
        );
        assert_eq!(
            host.persisted_movement_health
                .front()
                .map(|entry| entry.digest.as_str()),
            Some(persistent_digest(4).as_str())
        );
        assert_eq!(
            host.persisted_movement_health
                .back()
                .map(|entry| entry.digest.as_str()),
            Some(persistent_digest(19).as_str())
        );
        assert!(
            !host
                .persisted_movement_health
                .iter()
                .any(|entry| entry.digest == persistent_digest(3)),
            "the oldest entries were evicted at the cap"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_descriptor_validation_rejects_kind_abi_layout_and_null_processor() {
        let valid = MovementModuleV1 {
            magic: MOVEMENT_MAGIC,
            abi_version: ABI_VERSION,
            frame_size: std::mem::size_of::<MovementFrame>() as u32,
            output_size: std::mem::size_of::<MovementOutput>() as u32,
            process: Some(propose_step),
        };
        assert!(validate_movement_descriptor(&valid).is_ok());
        assert_eq!(
            validate_movement_descriptor(&MovementModuleV1 { magic: 1, ..valid }),
            Err(MovementRefusal::Kind)
        );
        assert_eq!(
            validate_movement_descriptor(&MovementModuleV1 {
                abi_version: ABI_VERSION + 1,
                ..valid
            }),
            Err(MovementRefusal::Kind)
        );
        assert_eq!(
            validate_movement_descriptor(&MovementModuleV1 {
                frame_size: 4,
                ..valid
            }),
            Err(MovementRefusal::Layout)
        );
        assert_eq!(
            validate_movement_descriptor(&MovementModuleV1 {
                process: None,
                ..valid
            }),
            Err(MovementRefusal::MissingProcessor)
        );
    }

    #[test]
    fn host_movement_seam_is_disabled_by_default() {
        let mut host = Host::new(std::env::temp_dir());
        assert!(
            !host.is_seam_enabled(ModuleKind::Movement),
            "gameplay seams default to disabled"
        );
        assert!(
            host.is_seam_enabled(ModuleKind::Pose),
            "the pose seam is always enabled"
        );
        // Disabling the pose seam is a no-op; it is not gated.
        host.set_seam_enabled(ModuleKind::Pose, false);
        assert!(host.is_seam_enabled(ModuleKind::Pose));
    }

    #[test]
    fn absent_movement_manifest_is_ok_false_while_the_seam_is_disabled() {
        let unique = format!(
            "rustwrath-hotpatch-absent-movement-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());
        assert!(!host.is_seam_enabled(ModuleKind::Movement));

        // A missing publication stays a non-failure even while the seam is disabled.
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert_eq!(host.take_movement_refusal(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn disabled_movement_seam_refuses_a_present_manifest_before_reading_the_artifact() {
        let unique = format!(
            "rustwrath-hotpatch-disabled-movement-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());
        host.observed = Some("pose-manifest".to_owned());
        // Leave a pending and an active candidate behind so the gate proves it drops both slots.
        host.movement_pending = Some(MovementLoaded::probe("rev-pending", propose_step));
        host.movement_active = Some(MovementLoaded::probe("rev-active", propose_step));
        assert!(host.has_movement_processor());
        assert!(host.movement_processor_active());

        let digest = "0123456789abcdef0123456789abcdef01234567";
        let dll_name = format!("movement-{digest}.dll");
        std::fs::write(
            root.join("movement.toml"),
            format!("revision = \"rev-m\"\nlibrary = \"{dll_name}\"\nsha1 = \"{digest}\"\n"),
        )
        .expect("write movement manifest");
        // The referenced DLL is deliberately absent: a gate placed after `MovementLoaded::open`
        // would report a publication error instead of `SeamDisabled`.
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::SeamDisabled)
        );
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::SeamDisabled)
        );
        assert_eq!(
            host.take_movement_rejection().as_deref(),
            Some("movement seam is disabled")
        );
        assert!(!host.has_movement_processor(), "the gate drops both slots");
        assert!(!host.movement_processor_active());
        assert!(host.movement_pending.is_none());
        assert!(host.movement_active.is_none());
        assert!(
            host.movement_observed.is_some(),
            "the refused manifest text stays latched so garbage collection pins its artifact"
        );
        assert_eq!(
            host.observed.as_deref(),
            Some("pose-manifest"),
            "the pose latch is independent of the movement gate"
        );

        // An unchanged repeat poll is a quiet `Ok(false)`; only the first call is a refusal. The
        // next poll after that still sees the same latch and stays quiet.
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert_eq!(host.take_movement_refusal(), None);
        assert!(!host.has_movement_processor());
        assert!(host.movement_observed.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn disabled_movement_manifest_still_pins_its_artifact_and_pose_collection_runs() {
        let unique = format!(
            "rustwrath-hotpatch-movement-gc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let digest = "0123456789abcdef0123456789abcdef01234567";
        let other = "89abcdef0123456789abcdef0123456789abcdef";
        let pinned = format!("movement-{digest}.dll");
        let retired = format!("movement-{other}.dll");
        let pose = format!("pose-{}.dll", "a".repeat(40));
        std::fs::write(
            root.join("movement.toml"),
            format!("revision = \"rev-m\"\nlibrary = \"{pinned}\"\nsha1 = \"{digest}\"\n"),
        )
        .expect("write movement manifest");
        for file in [&pinned, &retired, &pose] {
            std::fs::write(root.join(file), b"artifact").expect("write artifact");
        }

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::SeamDisabled)
        );
        assert!(
            root.join(&pinned).exists(),
            "the refused publication is still on disk before collection"
        );
        assert_eq!(host.collect_retired().expect("collection succeeds"), 2);
        assert!(
            root.join(&pinned).exists(),
            "the latched movement publication stays pinned while the seam is disabled"
        );
        assert!(!root.join(&retired).exists(), "unpinned movement is collected");
        assert!(!root.join(&pose).exists(), "pose collection still runs");
        assert!(!root.join("publish.lock").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn disabling_the_movement_seam_keeps_the_manifest_latched_for_collection() {
        let unique = format!(
            "rustwrath-hotpatch-movement-disable-gc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let digest = "0123456789abcdef0123456789abcdef01234567";
        let dll_name = format!("movement-{digest}.dll");
        let manifest = format!("revision = \"rev-m\"\nlibrary = \"{dll_name}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("movement.toml"), &manifest).expect("write movement manifest");
        std::fs::write(root.join(&dll_name), b"artifact").expect("write movement artifact");
        let pose = format!("pose-{}.dll", "a".repeat(40));
        std::fs::write(root.join(&pose), b"artifact").expect("write pose artifact");

        let mut host = Host::new(root.clone());
        // Simulate a prior successful stage without building a native DLL.
        host.seams.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_observed = Some(manifest);
        host.movement_active = Some(MovementLoaded::probe("rev-m", propose_step));
        assert!(host.movement_processor_active());

        host.set_seam_enabled(ModuleKind::Movement, false);
        assert!(!host.has_movement_processor(), "disable evicts the processor");
        assert!(
            host.movement_observed.is_some(),
            "disable keeps the manifest latched for retention"
        );
        assert_eq!(host.collect_retired().expect("collection succeeds"), 1);
        assert!(
            root.join(&dll_name).exists(),
            "the disabled publication stays pinned"
        );
        assert!(!root.join(&pose).exists(), "pose collection still runs");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn enabling_the_movement_seam_releases_the_same_present_manifest() {
        let unique = format!(
            "rustwrath-hotpatch-enabled-movement-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let digest = "0123456789abcdef0123456789abcdef01234567";
        std::fs::write(
            root.join("movement.toml"),
            format!(
                "revision = \"rev-m\"\nlibrary = \"movement-{digest}.dll\"\nsha1 = \"{digest}\"\n"
            ),
        )
        .expect("write movement manifest");
        std::fs::write(root.join(format!("movement-{digest}.dll")), b"wrong").expect("write dll");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::SeamDisabled)
        );
        // The same manifest text now proceeds past the gate to publication verification.
        host.set_seam_enabled(ModuleKind::Movement, true);
        assert!(host.is_seam_enabled(ModuleKind::Movement));
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert!(!host.has_movement_processor());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn disabling_the_movement_seam_evicts_an_active_or_staged_processor() {
        let mut host = probe_host(propose_step);
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged probe proposes");
        assert!(host.commit_movement_processor());
        assert!(host.movement_processor_active());

        host.set_seam_enabled(ModuleKind::Movement, false);
        assert!(!host.has_movement_processor(), "disable evicts the active processor");
        assert!(!host.movement_processor_active());
        assert_eq!(
            host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS),
            Err(MovementRefusal::SeamDisabled)
        );
        assert!(!host.commit_movement_processor(), "nothing can be promoted");
        assert!(!host.has_movement_processor());
    }

    #[test]
    fn disabling_between_proposal_and_commit_refuses_the_promotion() {
        let mut host = probe_host(propose_step);
        host.set_seam_enabled(ModuleKind::Movement, true);
        let _ = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged probe proposes");
        host.set_seam_enabled(ModuleKind::Movement, false);
        assert!(!host.commit_movement_processor());
        assert!(!host.movement_processor_active());
        assert!(!host.has_movement_processor());
    }

    #[test]
    fn movement_manifest_failure_leaves_the_pose_publication_untouched() {
        let unique = format!(
            "rustwrath-hotpatch-movement-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let mut host = Host::new(root.clone());
        host.observed = Some("pose-manifest".to_owned());

        // An absent movement manifest is not a failure and must not disturb the pose latch.
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert_eq!(host.observed.as_deref(), Some("pose-manifest"));

        // A present manifest only reaches publication verification once its seam is enabled.
        host.set_seam_enabled(ModuleKind::Movement, true);

        // A movement artifact whose bytes do not match its publication is a typed refusal.
        let digest = "0123456789abcdef0123456789abcdef01234567";
        let manifest = format!(
            "revision = \"rev-m\"\nlibrary = \"movement-{digest}.dll\"\nsha1 = \"{digest}\"\n"
        );
        std::fs::write(root.join("movement.toml"), &manifest).expect("write movement manifest");
        std::fs::write(root.join(format!("movement-{digest}.dll")), b"wrong").expect("write dll");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert!(
            !host.has_movement_processor(),
            "a failed publication stages nothing"
        );
        assert_eq!(
            host.observed.as_deref(),
            Some("pose-manifest"),
            "the pose publication latch is independent"
        );
        assert_eq!(
            host.movement_observed.as_deref(),
            Some(manifest.as_str()),
            "the deterministic movement failure latches its own text"
        );

        // Retention recognises both seams this host owns and nothing else, so a combat/space/story
        // artifact published beside them is never collected by this sweep.
        let pose = format!("pose-{}.dll", "a".repeat(40));
        let movement = format!("movement-{}.dll", "b".repeat(40));
        let combat = format!("combat-{}.dll", "c".repeat(40));
        assert!(is_managed_artifact(&pose, ModuleKind::Pose.prefix()));
        assert!(is_managed_artifact(
            &movement,
            ModuleKind::Movement.prefix()
        ));
        assert!(!is_managed_artifact(&combat, ModuleKind::Pose.prefix()));
        assert!(!is_managed_artifact(&combat, ModuleKind::Movement.prefix()));
        assert!(!is_managed_artifact(
            "movement-not-a-digest.dll",
            "movement-"
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn movement_test_root(tag: &str) -> std::path::PathBuf {
        let unique = format!(
            "rustwrath-hotpatch-movement-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn movement_publication_text(revision: &str, library: &str, sha1: &str) -> String {
        format!("revision = \"{revision}\"\nlibrary = \"{library}\"\nsha1 = \"{sha1}\"\n")
    }

    #[test]
    fn malformed_movement_manifest_latches_and_retries_when_rewritten() {
        let root = movement_test_root("malformed");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let malformed = "revision = \"unterminated\n";
        std::fs::write(root.join(MOVEMENT_MANIFEST), malformed).expect("write malformed manifest");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(malformed));
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::Publication)
        );
        let detail = host.take_movement_rejection().expect("loader detail");
        assert!(
            detail.contains("movement publication:"),
            "detail names the publication: {detail}"
        );
        assert!(
            detail.to_lowercase().contains("toml parse error"),
            "detail names the parse error: {detail}"
        );

        // The unchanged malformed bytes are not re-parsed and do not report again.
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert_eq!(host.take_movement_refusal(), None);

        // Rewriting a valid manifest clears the latch and retries. The absent DLL is the transient
        // `NotFound` path, so the manifest is not latched and no digest is quarantined.
        let digest = "0123456789abcdef0123456789abcdef01234567";
        let manifest =
            movement_publication_text("rev-m", &format!("movement-{digest}.dll"), digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write valid manifest");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert!(
            host.movement_observed.is_none(),
            "a transient missing DLL must not latch the movement manifest"
        );
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "a transient missing DLL must not quarantine"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn verified_non_pe_movement_artifacts_are_quarantined_and_refused_before_reloading() {
        let root = movement_test_root("quarantine");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let bytes_b = b"not a pe image B movement";
        let bytes_c = b"not a pe image C movement";
        let digest_b = format!("{:x}", Sha1::digest(bytes_b));
        let digest_c = format!("{:x}", Sha1::digest(bytes_c));
        let dll_b = format!("movement-{digest_b}.dll");
        let dll_c = format!("movement-{digest_c}.dll");
        let manifest_b = movement_publication_text("rev-b", &dll_b, &digest_b);
        let manifest_c = movement_publication_text("rev-c", &dll_c, &digest_c);

        // B fails after verification (`Library::new` on non-PE bytes) and is quarantined.
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest_b).expect("write B manifest");
        std::fs::write(root.join(&dll_b), bytes_b).expect("write B dll");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest_b.as_str()));
        assert!(
            host.is_movement_quarantined(&digest_b),
            "B's verified bytes are quarantined after the terminal load failure"
        );

        // C, also a verified digest with an invalid image, is quarantined independently.
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest_c).expect("write C manifest");
        std::fs::write(root.join(&dll_c), bytes_c).expect("write C dll");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest_c.as_str()));
        assert!(host.is_movement_quarantined(&digest_c), "C is quarantined");

        // Republish B with its DLL deleted. The quarantine gate must fire before any artifact
        // read, so this is not the transient missing-file path even though the file is absent.
        std::fs::remove_file(root.join(&dll_b)).expect("delete B dll");
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest_b).expect("republish B manifest");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Quarantined)
        );
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::Quarantined)
        );
        let detail = host.take_movement_rejection().expect("quarantine detail");
        assert!(detail.contains("quarantined"), "detail: {detail}");
        assert!(
            detail.contains(digest_b.as_str()),
            "detail names the quarantined digest: {detail}"
        );
        assert!(
            !detail.contains("No such file") && !detail.contains("os error 2"),
            "the quarantine refusal is not the missing-file path: {detail}"
        );
        assert!(
            !host.has_movement_processor(),
            "a quarantined digest stages nothing"
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest_b.as_str()));

        // The unchanged republished text is a quiet poll.
        assert_eq!(host.stage_movement_processor(), Ok(false));
        assert_eq!(host.take_movement_refusal(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_digest_mismatch_latches_without_quarantining_the_declared_digest() {
        let root = movement_test_root("mismatch");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let declared = "1111111111111111111111111111111111111111";
        let dll = format!("movement-{declared}.dll");
        let manifest = movement_publication_text("rev-b", &dll, declared);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write manifest");
        std::fs::write(root.join(&dll), b"bytes whose digest is not declared")
            .expect("write wrong bytes");

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        assert!(
            !host.is_movement_quarantined(declared),
            "an unverified declared digest must not be quarantined"
        );
        assert!(host.movement_quarantined_digests.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_wrong_immutable_filename_latches_without_quarantining_the_verified_bytes() {
        let root = movement_test_root("filename");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        // The bytes match the declared digest, but the manifest names a different immutable file.
        // That is a manifest/name failure, not proof the content is bad, so it must latch without
        // quarantining the digest.
        let bytes = b"movement bytes that hash to the declared digest";
        let digest = format!("{:x}", Sha1::digest(bytes));
        let wrong_name = format!("movement-{}.dll", "f".repeat(40));
        let manifest = movement_publication_text("rev-f", &wrong_name, &digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write manifest");
        std::fs::write(root.join(&wrong_name), bytes).expect("write artifact");

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        assert!(
            !host.is_movement_quarantined(&digest),
            "a filename-only failure must not block a correctly named republish"
        );
        assert!(host.movement_quarantined_digests.is_empty());

        // Republishing the same bytes under the correct immutable name passes verification, so
        // the next failure is the post-verification `Library::new` on non-PE bytes. Only then is
        // the proved digest quarantined.
        let correct_name = format!("movement-{digest}.dll");
        let republished = movement_publication_text("rev-f2", &correct_name, &digest);
        std::fs::write(root.join(&correct_name), bytes).expect("write correctly named artifact");
        std::fs::write(root.join(MOVEMENT_MANIFEST), &republished).expect("republish manifest");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(republished.as_str()));
        assert!(
            host.is_movement_quarantined(&digest),
            "the post-verification load failure now quarantines the proved digest"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_movement_dll_is_retried_without_latching_or_quarantining() {
        let root = movement_test_root("missing");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let digest = "3333333333333333333333333333333333333333";
        let dll = format!("movement-{digest}.dll");
        let manifest = movement_publication_text("rev-a", &dll, digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write manifest");

        for _ in 0..2 {
            assert_eq!(
                host.stage_movement_processor(),
                Err(MovementRefusal::Publication)
            );
            assert!(
                host.movement_observed.is_none(),
                "a transient missing DLL must not latch the movement manifest"
            );
            assert!(
                host.movement_quarantined_digests.is_empty(),
                "a transient missing DLL has no proved digest to quarantine"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_publish_lock_keeps_a_post_verification_failure_retryable() {
        let root = movement_test_root("publish-lock");
        let bytes = b"not a pe movement image behind the lock";
        let digest = format!("{:x}", Sha1::digest(bytes));
        let dll = format!("movement-{digest}.dll");
        let manifest = movement_publication_text("rev-a", &dll, &digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write manifest");
        std::fs::write(root.join(&dll), bytes).expect("write artifact");
        std::fs::write(root.join("publish.lock"), b"").expect("write lock");

        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert!(
            host.movement_observed.is_none(),
            "the publisher-held artifact must not latch"
        );
        assert!(
            !host.is_movement_quarantined(&digest),
            "and it must not quarantine a possibly incomplete copy"
        );

        std::fs::remove_file(root.join("publish.lock")).expect("remove lock");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        assert!(
            host.is_movement_quarantined(&digest),
            "after the lock clears the proved digest is quarantined"
        );
        assert_eq!(host.stage_movement_processor(), Ok(false));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_quarantine_store_dedupes_and_is_bounded() {
        let mut host = Host::new(std::env::temp_dir());
        host.quarantine_movement_digest("dup");
        host.quarantine_movement_digest("dup");
        assert_eq!(
            host.movement_quarantined_digests.len(),
            1,
            "a repeated digest must not duplicate"
        );
        assert!(host.is_movement_quarantined("dup"));

        for index in 0..MAX_QUARANTINED_DIGESTS {
            host.quarantine_movement_digest(&format!("digest-{index:02}"));
        }
        assert_eq!(
            host.movement_quarantined_digests.len(),
            MAX_QUARANTINED_DIGESTS,
            "the movement store is capped"
        );
        assert!(
            !host.is_movement_quarantined("dup"),
            "the oldest entry is evicted at the cap"
        );
        assert!(host.is_movement_quarantined("digest-00"));
        assert!(host.is_movement_quarantined(&format!(
            "digest-{:02}",
            MAX_QUARANTINED_DIGESTS - 1
        )));
        assert!(
            host.quarantined_digests.is_empty(),
            "the pose quarantine store stays independent"
        );
    }

    #[test]
    fn terminal_movement_latch_pins_the_refused_artifact_and_still_collects_retired() {
        let root = movement_test_root("retention");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let declared = "5555555555555555555555555555555555555555";
        let declared_dll = format!("movement-{declared}.dll");
        let manifest = movement_publication_text("rev-m", &declared_dll, declared);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write manifest");
        std::fs::write(
            root.join(&declared_dll),
            b"bytes that do not match the declared digest",
        )
        .expect("write artifact");

        let retired = format!("movement-{}.dll", "6".repeat(40));
        let pose = format!("pose-{}.dll", "7".repeat(40));
        for file in [&retired, &pose] {
            std::fs::write(root.join(file), b"artifact").expect("write retired artifact");
        }

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        assert_eq!(
            host.collect_retired().expect("collection succeeds"),
            2,
            "the latched movement manifest lets the sweep run"
        );
        assert!(
            root.join(&declared_dll).exists(),
            "the current publication stays pinned"
        );
        assert!(
            !root.join(&retired).exists(),
            "an unpinned movement artifact is collected"
        );
        assert!(!root.join(&pose).exists(), "pose collection still runs");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_quarantine_survives_disable_enable_and_re_arms_the_refusal() {
        let root = movement_test_root("quarantine-toggle");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let digest = "8888888888888888888888888888888888888888";
        let dll = format!("movement-{digest}.dll");
        let manifest = movement_publication_text("rev-q", &dll, digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write manifest");

        // Simulate a digest quarantined by a prior terminal post-verification failure. The gate
        // must fire before verification, so the absent DLL is irrelevant.
        host.quarantine_movement_digest(digest);
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Quarantined)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        assert!(
            host.movement_failure_stamp.is_none(),
            "the quarantine gate records no artifact stamp"
        );

        // Pin a synthetic stamp so the disable/enable retention rule is observable.
        host.movement_failure_stamp = Some(ArtifactStamp {
            library: dll.clone(),
            len: 0,
            modified: None,
        });
        host.set_seam_enabled(ModuleKind::Movement, false);
        assert!(
            host.is_movement_quarantined(digest),
            "disabling the seam must not clear the quarantine"
        );
        assert!(
            host.movement_failure_stamp.is_some(),
            "disabling keeps the stamp with the manifest latch for retention"
        );
        host.set_seam_enabled(ModuleKind::Movement, true);
        assert!(
            host.is_movement_quarantined(digest),
            "enabling the seam must not clear the quarantine"
        );
        assert!(
            host.movement_observed.is_none(),
            "enable clears the manifest latch so the same text is re-evaluated"
        );
        assert!(
            host.movement_failure_stamp.is_none(),
            "enable clears the artifact stamp with the latch"
        );

        // Re-enabling re-arms exactly one refusal, then the unchanged text stays quiet.
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Quarantined)
        );
        assert_eq!(host.stage_movement_processor(), Ok(false));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn terminal_movement_failure_retains_the_active_processor() {
        let root = movement_test_root("active-retention");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe("rev-active", propose_step));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged probe proposes");
        assert!(host.commit_movement_processor());
        assert!(host.movement_processor_active());

        let digest = "9999999999999999999999999999999999999999";
        let dll = format!("movement-{digest}.dll");
        let manifest = movement_publication_text("rev-bad", &dll, digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write manifest");
        std::fs::write(root.join(&dll), b"wrong bytes").expect("write artifact");

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert!(host.movement_pending.is_none());
        assert!(
            host.movement_processor_active(),
            "a failed candidate must not evict the active processor"
        );
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the retained active probe still proposes");
        assert_eq!(source, MovementProposalSource::Active);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Same-text artifact repair. A terminal failure latches the manifest text; the artifact stamp
    // lets an identical-text call retry once when the file behind that text changed. These cases
    // use non-PE bytes and file metadata only, so no native library is built or loaded.
    // ------------------------------------------------------------------

    #[test]
    fn pose_same_text_repair_retries_after_the_artifact_changes() {
        let root = pose_test_root("same-text-repair");
        let mut host = Host::new(root.clone());

        let repaired_bytes = b"not a pe image after an in-place repair";
        let digest = format!("{:x}", Sha1::digest(repaired_bytes));
        let dll = format!("pose-{digest}.dll");
        let manifest =
            format!("revision = \"rev-repair\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");

        // The first artifact is shorter and does not hash to the declared digest, so verification
        // fails terminally and records the artifact stamp without quarantining the declared digest.
        let wrong_bytes = b"short";
        assert_ne!(wrong_bytes.len(), repaired_bytes.len());
        std::fs::write(root.join(&dll), wrong_bytes).expect("write wrong bytes");
        let first = host.poll().expect_err("the digest mismatch is terminal");
        assert!(first
            .to_string()
            .contains("module digest differs from publication"));
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        let first_stamp = host.pose_failure_stamp.clone();
        assert!(
            first_stamp.is_some(),
            "a terminal pose failure records the artifact stamp"
        );
        assert!(
            !host.is_quarantined(&digest),
            "a verification failure never quarantines the declared digest"
        );

        // A different still-bad artifact length clears the latch and re-latches with a fresh stamp;
        // an unchanged repeat is then quiet, proving the retry is not a permanent Err loop.
        let still_wrong_bytes = b"still the wrong artifact bytes";
        assert_ne!(still_wrong_bytes.len(), wrong_bytes.len());
        std::fs::write(root.join(&dll), still_wrong_bytes).expect("write different wrong bytes");
        let still_bad = host
            .poll()
            .expect_err("the changed still-bad artifact is retried");
        assert!(still_bad
            .to_string()
            .contains("module digest differs from publication"));
        assert_ne!(
            host.pose_failure_stamp, first_stamp,
            "the re-latched failure records the new artifact stamp"
        );
        let second_stamp = host.pose_failure_stamp.clone();
        assert!(!host.poll().expect("the unchanged still-bad artifact stays quiet"));
        assert_eq!(
            host.pose_failure_stamp, second_stamp,
            "the fresh stamp survives an unchanged repeat"
        );

        // Repair in place without touching ready.toml. The artifact length changed, so the stamp
        // mismatches and the identical manifest text is retried: verification passes and the
        // non-PE bytes fail inside `Loaded::open`, which quarantines the now-proved digest.
        std::fs::write(root.join(&dll), repaired_bytes).expect("replace with repaired bytes");
        let second = host.poll();
        assert!(
            second.is_err(),
            "a changed artifact must not be a quiet Ok(false): {second:?}"
        );
        let second = second.expect_err("the repaired artifact reaches loading");
        assert!(
            second.to_string().contains("loading presentation module"),
            "the retry must reach the load failure, got: {second:#}"
        );
        assert!(
            host.is_quarantined(&digest),
            "the repaired bytes were verified before the load failure"
        );
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert!(
            host.pose_failure_stamp.is_some(),
            "the load failure records the repaired artifact stamp again"
        );
        assert!(
            !host
                .poll()
                .expect("the unchanged repaired artifact stays quiet"),
            "the re-latched load failure must be quiet on the next unchanged poll"
        );
        assert!(
            host.pose_failure_stamp.is_some(),
            "the unchanged repeat keeps the load-failure stamp"
        );

        // A later byte change to a quarantined post-verification failure clears the latch and
        // produces exactly one quarantine refusal before re-latching; the gate records no stamp.
        let mut changed_after_quarantine = repaired_bytes.to_vec();
        changed_after_quarantine.push(b'x');
        std::fs::write(root.join(&dll), &changed_after_quarantine)
            .expect("modify the quarantined artifact");
        let quarantined = host
            .poll()
            .expect_err("the quarantined digest is refused before loading");
        assert!(
            quarantined.to_string().contains("quarantined"),
            "expected a quarantine refusal, got: {quarantined:#}"
        );
        assert!(
            host.pose_failure_stamp.is_none(),
            "the quarantine gate records no artifact stamp"
        );
        assert!(!host
            .poll()
            .expect("the re-latched quarantine text is quiet"));
        assert!(
            host.pose_failure_stamp.is_none(),
            "the gate latch stays text-only"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pose_unchanged_artifact_stays_latched_after_terminal_failure() {
        let root = pose_test_root("unchanged-artifact");
        let mut host = Host::new(root.clone());

        let digest = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let dll = format!("pose-{digest}.dll");
        let manifest =
            format!("revision = \"rev-static\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        std::fs::write(root.join(&dll), b"bytes that do not match the declared digest")
            .expect("write wrong bytes");

        let first = host.poll().expect_err("the digest mismatch is terminal");
        assert!(first
            .to_string()
            .contains("module digest differs from publication"));
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        let stamp = host.pose_failure_stamp.clone();
        assert!(stamp.is_some(), "the terminal failure records a stamp");

        // No artifact change: the same text and the same file stay a quiet poll.
        assert!(
            !host.poll().expect("the unchanged artifact stays quiet"),
            "an unchanged artifact must not re-report"
        );
        assert_eq!(
            host.pose_failure_stamp, stamp,
            "the stamp survives an unchanged repeat"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pose_malformed_manifest_leaves_no_artifact_stamp() {
        let root = pose_test_root("malformed-no-stamp");
        let mut host = Host::new(root.clone());

        let malformed = "revision = \"unterminated\n";
        std::fs::write(root.join("ready.toml"), malformed).expect("write malformed ready.toml");
        let error = host
            .poll()
            .expect_err("malformed TOML is a terminal publication error");
        assert!(!transient_io(&error));
        assert_eq!(host.observed.as_deref(), Some(malformed));
        assert!(
            host.pose_failure_stamp.is_none(),
            "malformed TOML has no trustworthy publication to name an artifact"
        );
        assert!(!host
            .poll()
            .expect("the latched malformed manifest stays quiet"));
        assert!(host.pose_failure_stamp.is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pose_same_length_repair_with_a_changed_mtime_retries() {
        let root = pose_test_root("mtime-repair");
        let declared = "7777777777777777777777777777777777777777";
        let dll = format!("pose-{declared}.dll");
        let manifest =
            format!("revision = \"rev-m\"\nlibrary = \"{dll}\"\nsha1 = \"{declared}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        let path = root.join(&dll);
        std::fs::write(&path, b"0123456789abcdef").expect("write first bad bytes");
        let mut host = Host::new(root.clone());
        let _ = host
            .poll()
            .expect_err("the first mismatched artifact is terminal");
        let stamp = host.pose_failure_stamp.as_ref().expect("stamp recorded");
        assert!(
            stamp.modified.is_some(),
            "the test filesystem must report a modification time"
        );
        let next_modified = stamp
            .modified
            .map(|modified| modified + std::time::Duration::from_secs(10))
            .unwrap_or(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            );

        // Keep the same length so only the modification time can distinguish the repair.
        std::fs::write(&path, b"fedcba9876543210").expect("write same-length bad bytes");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open artifact");
        file.set_times(std::fs::FileTimes::new().set_modified(next_modified))
            .expect("set artifact mtime");
        drop(file);

        let error = host
            .poll()
            .expect_err("the changed mtime clears the latch and retries");
        assert!(
            error.to_string().contains("module digest differs"),
            "the same-length bad bytes are re-verified: {error:#}"
        );
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert!(
            host.pose_failure_stamp.is_some(),
            "the retry re-latches with a fresh stamp"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pose_missing_artifact_retries_without_quarantine_and_skips_the_sweep() {
        let root = pose_test_root("missing-repair");
        let correct = b"recreated non-pe pose artifact";
        let digest = format!("{:x}", Sha1::digest(correct));
        let dll = format!("pose-{digest}.dll");
        let manifest = format!("revision = \"rev-r\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        std::fs::write(root.join(&dll), b"wrong").expect("write wrong bytes");
        let mut host = Host::new(root.clone());
        let _ = host
            .poll()
            .expect_err("the mismatched artifact is terminal");
        assert!(host.pose_failure_stamp.is_some());

        let retired = format!("pose-{}.dll", "a".repeat(40));
        std::fs::write(root.join(&retired), b"retired").expect("write retired artifact");
        std::fs::remove_file(root.join(&dll)).expect("delete artifact");
        let error = host
            .poll()
            .expect_err("the missing artifact clears the latch and retries");
        assert!(
            transient_io(&error),
            "the missing artifact reaches the transient path: {error:#}"
        );
        assert!(host.observed.is_none());
        assert!(host.pose_failure_stamp.is_none());
        assert_eq!(
            host.collect_retired().expect("collection succeeds"),
            0,
            "the sweep has no latch to anchor while the manifest is unlatched"
        );
        assert!(
            root.join(&retired).exists(),
            "the sweep is skipped, not run with an unlatched manifest"
        );

        std::fs::write(root.join(&dll), correct).expect("write repaired artifact");
        let error = host
            .poll()
            .expect_err("the repaired artifact reaches Loaded::open");
        assert!(error.to_string().contains("loading presentation module"));
        assert!(host.is_quarantined(&digest));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pose_same_mtime_length_change_retries() {
        let root = pose_test_root("length-repair");
        let declared = "9999999999999999999999999999999999999999";
        let dll = format!("pose-{declared}.dll");
        let manifest =
            format!("revision = \"rev-l\"\nlibrary = \"{dll}\"\nsha1 = \"{declared}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        let path = root.join(&dll);
        std::fs::write(&path, b"short").expect("write first bad bytes");
        let mut host = Host::new(root.clone());
        let _ = host
            .poll()
            .expect_err("the first mismatched artifact is terminal");
        let stamp = host.pose_failure_stamp.as_ref().expect("stamp recorded");
        let modified = stamp
            .modified
            .expect("the test filesystem must report a modification time");

        // Change only the length and force the original mtime back.
        std::fs::write(&path, b"a longer still-bad artifact").expect("write longer bad bytes");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open artifact");
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .expect("restore artifact mtime");
        drop(file);

        let error = host
            .poll()
            .expect_err("the changed length clears the latch and retries");
        assert!(
            error.to_string().contains("module digest differs"),
            "the longer bad bytes are re-verified: {error:#}"
        );
        assert!(host.pose_failure_stamp.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pose_quarantined_missing_artifact_relatches_at_the_gate_and_allows_the_sweep() {
        let root = pose_test_root("quarantined-missing");
        let correct = b"quarantined non-pe pose artifact";
        let digest = format!("{:x}", Sha1::digest(correct));
        let dll = format!("pose-{digest}.dll");
        let manifest = format!("revision = \"rev-q\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");
        std::fs::write(root.join(&dll), correct).expect("write non-pe artifact");
        let mut host = Host::new(root.clone());
        let error = host
            .poll()
            .expect_err("the non-pe artifact fails after verification");
        assert!(error.to_string().contains("loading presentation module"));
        assert!(host.is_quarantined(&digest));
        assert!(host.pose_failure_stamp.is_some());

        let retired = format!("pose-{}.dll", "e".repeat(40));
        std::fs::write(root.join(&retired), b"retired").expect("write retired artifact");
        std::fs::remove_file(root.join(&dll)).expect("delete artifact");
        let error = host
            .poll()
            .expect_err("the missing quarantined artifact is refused at the gate");
        assert!(
            error.to_string().contains("quarantined"),
            "the gate runs before the missing-file read: {error:#}"
        );
        assert_eq!(host.observed.as_deref(), Some(manifest.as_str()));
        assert!(host.pose_failure_stamp.is_none());
        assert_eq!(
            host.collect_retired().expect("collection succeeds"),
            1,
            "the gate latch lets the sweep run"
        );
        assert!(!root.join(&retired).exists());
        assert!(!host
            .poll()
            .expect("the latched quarantine text stays quiet"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn artifact_stamp_requires_a_basename_inside_the_publication_root() {
        let root = pose_test_root("stamp-basename");
        assert!(
            artifact_stamp(&root, "pose-missing.dll").is_none(),
            "an absent artifact has no stamp"
        );
        std::fs::write(root.join("inside.dll"), b"artifact").expect("write inside artifact");
        assert!(
            artifact_stamp(&root, "inside.dll").is_some(),
            "a plain basename inside the root is stamped"
        );
        assert!(
            artifact_stamp(&root, "subdir/inside.dll").is_none(),
            "a nested path is refused before it can stat outside the root"
        );
        assert!(
            artifact_stamp(&root, "../inside.dll").is_none(),
            "a parent-directory path is refused"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_same_text_repair_retries_after_the_artifact_changes() {
        let root = movement_test_root("same-text-repair");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let repaired_bytes = b"not a pe movement image after an in-place repair";
        let digest = format!("{:x}", Sha1::digest(repaired_bytes));
        let dll = format!("movement-{digest}.dll");
        let manifest = movement_publication_text("rev-repair", &dll, &digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement manifest");

        let wrong_bytes = b"short";
        assert_ne!(wrong_bytes.len(), repaired_bytes.len());
        std::fs::write(root.join(&dll), wrong_bytes).expect("write wrong bytes");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        let first_stamp = host.movement_failure_stamp.clone();
        assert!(
            first_stamp.is_some(),
            "a terminal movement failure records the artifact stamp"
        );
        assert!(
            !host.is_movement_quarantined(&digest),
            "a verification failure never quarantines the declared digest"
        );

        // A different still-bad artifact length clears the latch and re-latches with a fresh stamp;
        // an unchanged repeat is then quiet.
        let still_wrong_bytes = b"still the wrong movement artifact bytes";
        assert_ne!(still_wrong_bytes.len(), wrong_bytes.len());
        std::fs::write(root.join(&dll), still_wrong_bytes).expect("write different wrong bytes");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_ne!(
            host.movement_failure_stamp, first_stamp,
            "the re-latched failure records the new artifact stamp"
        );
        let second_stamp = host.movement_failure_stamp.clone();
        assert_eq!(
            host.stage_movement_processor(),
            Ok(false),
            "the unchanged still-bad artifact stays quiet"
        );
        assert_eq!(
            host.movement_failure_stamp, second_stamp,
            "the fresh stamp survives an unchanged repeat"
        );

        // Repair in place without touching movement.toml. The changed length clears the latch, so
        // the same text is retried: verification passes and the non-PE bytes fail in
        // `MovementLoaded::open`, which quarantines the now-proved digest.
        std::fs::write(root.join(&dll), repaired_bytes).expect("replace with repaired bytes");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication),
            "a changed artifact must not be a quiet Ok(false)"
        );
        let detail = host.take_movement_rejection().expect("loader detail");
        assert!(
            detail.contains("loading presentation module"),
            "the retry must reach the load failure, got: {detail}"
        );
        assert!(
            host.is_movement_quarantined(&digest),
            "the repaired bytes were verified before the load failure"
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        assert!(host.movement_failure_stamp.is_some());
        assert_eq!(
            host.stage_movement_processor(),
            Ok(false),
            "the re-latched load failure must be quiet on the next unchanged poll"
        );
        assert!(host.movement_failure_stamp.is_some());

        // A later byte change to a quarantined post-verification failure clears the latch and
        // produces exactly one quarantine refusal before re-latching; the gate records no stamp.
        let mut changed_after_quarantine = repaired_bytes.to_vec();
        changed_after_quarantine.push(b'x');
        std::fs::write(root.join(&dll), &changed_after_quarantine)
            .expect("modify the quarantined artifact");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Quarantined)
        );
        assert_eq!(
            host.take_movement_refusal(),
            Some(MovementRefusal::Quarantined)
        );
        assert!(
            host.movement_failure_stamp.is_none(),
            "the quarantine gate records no artifact stamp"
        );
        assert_eq!(
            host.stage_movement_processor(),
            Ok(false),
            "the re-latched quarantine text is quiet"
        );
        assert!(
            host.movement_failure_stamp.is_none(),
            "the gate latch stays text-only"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_unchanged_artifact_stays_latched_after_terminal_failure() {
        let root = movement_test_root("unchanged-artifact");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);

        let digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let dll = format!("movement-{digest}.dll");
        let manifest = movement_publication_text("rev-static", &dll, digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement manifest");
        std::fs::write(root.join(&dll), b"bytes that do not match the declared digest")
            .expect("write wrong bytes");

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert_eq!(host.movement_observed.as_deref(), Some(manifest.as_str()));
        let stamp = host.movement_failure_stamp.clone();
        assert!(stamp.is_some(), "the terminal failure records a stamp");

        assert_eq!(
            host.stage_movement_processor(),
            Ok(false),
            "an unchanged artifact must not re-report"
        );
        assert_eq!(
            host.movement_failure_stamp, stamp,
            "the stamp survives an unchanged repeat"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_same_length_repair_with_a_changed_mtime_retries() {
        let root = movement_test_root("mtime-repair");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        let declared = "cccccccccccccccccccccccccccccccccccccccc";
        let dll = format!("movement-{declared}.dll");
        let manifest = movement_publication_text("rev-m", &dll, declared);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement manifest");
        let path = root.join(&dll);
        std::fs::write(&path, b"0123456789abcdef").expect("write first bad bytes");

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        let stamp = host
            .movement_failure_stamp
            .as_ref()
            .expect("stamp recorded");
        assert!(
            stamp.modified.is_some(),
            "the test filesystem must report a modification time"
        );
        let next_modified = stamp
            .modified
            .map(|modified| modified + std::time::Duration::from_secs(10))
            .unwrap_or(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            );

        // Keep the same length so only the modification time can distinguish the repair.
        std::fs::write(&path, b"fedcba9876543210").expect("write same-length bad bytes");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open artifact");
        file.set_times(std::fs::FileTimes::new().set_modified(next_modified))
            .expect("set artifact mtime");
        drop(file);

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication),
            "the changed mtime clears the latch and retries"
        );
        assert_eq!(
            host.movement_observed.as_deref(),
            Some(manifest.as_str()),
            "the retry re-latches the same manifest text"
        );
        let detail = host
            .take_movement_rejection()
            .expect("the retried bytes are re-verified");
        assert!(
            detail.contains("module digest differs"),
            "the same-length bad bytes are re-verified: {detail}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_missing_artifact_retries_without_quarantine_and_skips_the_sweep() {
        let root = movement_test_root("missing-repair");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        let correct = b"recreated non-pe movement artifact";
        let digest = format!("{:x}", Sha1::digest(correct));
        let dll = format!("movement-{digest}.dll");
        let manifest = movement_publication_text("rev-r", &dll, &digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement manifest");
        std::fs::write(root.join(&dll), b"wrong").expect("write wrong bytes");

        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication)
        );
        assert!(host.movement_failure_stamp.is_some());

        let retired = format!("movement-{}.dll", "d".repeat(40));
        std::fs::write(root.join(&retired), b"retired").expect("write retired artifact");
        std::fs::remove_file(root.join(&dll)).expect("delete artifact");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication),
            "the missing artifact clears the latch and retries"
        );
        assert!(
            host.movement_observed.is_none(),
            "the transient missing-file path leaves the latch unset"
        );
        assert!(host.movement_failure_stamp.is_none());
        assert_eq!(
            host.collect_retired().expect("collection succeeds"),
            0,
            "the sweep has no latch to anchor while the manifest is unlatched"
        );
        assert!(
            root.join(&retired).exists(),
            "the sweep is skipped, not run with an unlatched manifest"
        );

        std::fs::write(root.join(&dll), correct).expect("write repaired artifact");
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication),
            "the repaired artifact reaches MovementLoaded::open"
        );
        let detail = host
            .take_movement_rejection()
            .expect("the repaired bytes reach loading");
        assert!(
            detail.contains("loading presentation module"),
            "the repaired bytes reach the post-verification load: {detail}"
        );
        assert!(host.is_movement_quarantined(&digest));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Persistent digest quarantine (`<root>/quarantine.toml`). These cases exercise the file shape,
    // entry validation, merge order, cap/eviction, best-effort replacement and health-facing counts
    // with in-process hosts; no native library is built or loaded.
    // ------------------------------------------------------------------

    /// A distinct valid 40-hex digest for `index` (zero-padded, so ordering is readable).
    fn persistent_digest(index: usize) -> String {
        format!("{index:040x}")
    }

    fn quarantine_text(pose: &[String], movement: &[String]) -> String {
        toml::to_string(&QuarantineFile {
            pose: pose.to_vec(),
            movement: movement.to_vec(),
        })
        .expect("serialize quarantine fixture")
    }

    #[test]
    fn persistent_quarantine_rejects_an_oversized_file() {
        let root = pose_test_root("quarantine-oversized");
        std::fs::write(
            root.join(QUARANTINE_FILE),
            vec![b'a'; (MAX_QUARANTINE_FILE_BYTES + 1) as usize],
        )
        .expect("write oversized store");
        let mut host = Host::new(root.clone());
        let error = host
            .load_persistent_quarantine()
            .expect_err("an oversized store is rejected before reading");
        assert!(error.to_string().contains("exceeds"), "{error:#}");
        assert!(host.quarantined_digests.is_empty());
        assert!(host.movement_quarantined_digests.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_quarantine_round_trips_pose_and_movement_digests() {
        let root = pose_test_root("quarantine-round-trip");
        let mut host = Host::new(root.clone());
        let pose_a = persistent_digest(1);
        let pose_b = persistent_digest(2);
        let movement_a = persistent_digest(3);
        host.quarantine_digest(&pose_a);
        host.quarantine_digest(&pose_b);
        host.quarantine_movement_digest(&movement_a);

        host.persist_quarantine().expect("persist quarantine store");
        assert!(root.join(QUARANTINE_FILE).exists());

        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored
                .load_persistent_quarantine()
                .expect("load quarantine store"),
            3,
            "all three digests are new to the fresh host"
        );
        assert_eq!(
            restored
                .quarantined_digests
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![pose_a.clone(), pose_b.clone()],
            "the pose store keeps oldest-first order"
        );
        assert_eq!(
            restored
                .movement_quarantined_digests
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![movement_a.clone()]
        );
        assert!(restored.is_quarantined(&pose_b));
        assert!(restored.is_movement_quarantined(&movement_a));
        assert!(
            !restored.is_quarantined(&movement_a),
            "the pose and movement stores stay separate"
        );
        assert_eq!(
            restored.health_snapshot().quarantined_digests,
            restored.quarantined_digests.len(),
            "the health snapshot counts the restored pose store"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_quarantine_missing_file_is_an_empty_success() {
        let root = pose_test_root("quarantine-missing");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_quarantine()
                .expect("a missing store is not an error"),
            0
        );
        assert!(host.quarantined_digests.is_empty());
        assert!(host.movement_quarantined_digests.is_empty());
        host.persist_quarantine()
            .expect("an empty store persists as a no-op");
        assert!(
            !root.join(QUARANTINE_FILE).exists(),
            "an empty store never creates the file"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_quarantine_malformed_file_is_an_error_and_keeps_both_stores() {
        let root = pose_test_root("quarantine-malformed");
        let mut host = Host::new(root.clone());
        let existing_pose = persistent_digest(7);
        let existing_movement = persistent_digest(8);
        host.quarantine_digest(&existing_pose);
        host.quarantine_movement_digest(&existing_movement);

        std::fs::write(root.join(QUARANTINE_FILE), "pose = 1\n")
            .expect("write malformed store");
        assert!(
            host.load_persistent_quarantine().is_err(),
            "a document of the wrong shape must be reported, not silently ignored"
        );
        assert_eq!(host.quarantined_digests.len(), 1);
        assert_eq!(host.movement_quarantined_digests.len(), 1);
        assert!(host.is_quarantined(&existing_pose));
        assert!(host.is_movement_quarantined(&existing_movement));

        // The fixed shape rejects an otherwise well-formed document with an extra key.
        std::fs::write(
            root.join(QUARANTINE_FILE),
            format!("pose = [\"{existing_pose}\"]\nrevision = \"rev-a\"\n"),
        )
        .expect("write unknown-field store");
        assert!(host.load_persistent_quarantine().is_err());
        assert_eq!(host.quarantined_digests.len(), 1);
        assert_eq!(host.movement_quarantined_digests.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_quarantine_skips_invalid_and_duplicate_entries() {
        let root = pose_test_root("quarantine-invalid");
        let mut host = Host::new(root.clone());
        let valid = persistent_digest(0xabc);
        let upper = valid.to_ascii_uppercase();
        let nested = format!("modules/pose-{valid}.dll");
        let overflow = "a".repeat(48);
        let non_hex = format!("{}z", "0".repeat(39));
        let text = toml::to_string(&QuarantineFile {
            pose: vec![
                String::new(),
                "abc123".to_owned(),
                non_hex,
                nested,
                "../quarantine.toml".to_owned(),
                valid.clone(),
                valid.clone(),
                upper,
                overflow,
            ],
            movement: vec![valid.clone()],
        })
        .expect("serialize invalid-entry fixture");
        std::fs::write(root.join(QUARANTINE_FILE), text).expect("write store");

        assert_eq!(
            host.load_persistent_quarantine()
                .expect("invalid entries are skipped, not fatal"),
            2,
            "one unique pose digest plus one movement digest"
        );
        assert_eq!(host.quarantined_digests.len(), 1);
        assert_eq!(
            host.quarantined_digests.front().map(String::as_str),
            Some(valid.as_str()),
            "the accepted entry is normalized to lowercase"
        );
        assert_eq!(host.movement_quarantined_digests.len(), 1);
        assert!(
            host.is_quarantined(&valid),
            "the loaded digest is addressable in its lowercase form"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_quarantine_load_respects_the_cap_and_oldest_eviction() {
        let root = pose_test_root("quarantine-cap");
        let digests: Vec<String> = (0..40).map(persistent_digest).collect();
        std::fs::write(
            root.join(QUARANTINE_FILE),
            quarantine_text(&digests, &[]),
        )
        .expect("write store");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_quarantine().expect("load store"),
            MAX_QUARANTINED_DIGESTS,
            "an oversized list keeps only its newest cap-sized window"
        );
        assert_eq!(host.quarantined_digests.len(), MAX_QUARANTINED_DIGESTS);
        for (offset, digest) in digests[8..].iter().enumerate() {
            assert_eq!(
                host.quarantined_digests[offset], *digest,
                "the store holds the newest 32 entries oldest-first"
            );
        }
        assert!(
            !host.is_quarantined(&digests[7]),
            "the oldest entries were evicted at the cap"
        );
        assert!(host.is_quarantined(&digests[39]));
        assert!(host.movement_quarantined_digests.is_empty());

        // Reloading the same oversized file must not rotate the window with its evicted prefix.
        let window: Vec<String> = host.quarantined_digests.iter().cloned().collect();
        assert_eq!(
            host.load_persistent_quarantine().expect("second load"),
            0,
            "the second load inserts nothing"
        );
        assert_eq!(
            host.quarantined_digests.iter().cloned().collect::<Vec<_>>(),
            window,
            "the cap-sized window is unchanged by a reload"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persisted_quarantine_contains_only_digests_and_reloads_idempotently() {
        let root = pose_test_root("quarantine-content");
        let mut host = Host::new(root.clone());
        let pose = persistent_digest(11);
        let movement = persistent_digest(12);
        host.quarantine_digest(&pose);
        host.quarantine_movement_digest(&movement);
        host.persist_quarantine().expect("persist store");

        let text =
            std::fs::read_to_string(root.join(QUARANTINE_FILE)).expect("the store is readable");
        assert!(text.contains(&pose));
        assert!(text.contains(&movement));
        assert!(
            !text.contains(".dll"),
            "no immutable artifact names are written: {text}"
        );
        assert!(
            !text.contains("rev"),
            "no revision labels are written: {text}"
        );
        assert!(
            !text.contains('\\') && !text.contains('/'),
            "no paths are written: {text}"
        );
        assert!(
            !root.join(QUARANTINE_TEMP).exists(),
            "the temp file is renamed over the target, not left behind"
        );

        let file: QuarantineFile = toml::from_str(&text).expect("the store is valid TOML");
        assert_eq!(file.pose, vec![pose.clone()]);
        assert_eq!(file.movement, vec![movement.clone()]);
        for entry in file.pose.iter().chain(file.movement.iter()) {
            assert_eq!(entry.len(), 40, "only 40-hex digests are written: {entry}");
            assert!(
                entry
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
                "only lowercase hex is written: {entry}"
            );
        }

        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored.load_persistent_quarantine().expect("load store"),
            2
        );
        assert_eq!(
            restored
                .load_persistent_quarantine()
                .expect("a second load is a no-op"),
            0,
            "reloading the same file inserts nothing"
        );
        assert_eq!(
            restored.quarantined_digests.iter().cloned().collect::<Vec<_>>(),
            vec![pose]
        );
        assert_eq!(
            restored
                .movement_quarantined_digests
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![movement]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_quarantine_merges_into_existing_stores_without_clearing() {
        let root = pose_test_root("quarantine-merge");
        let mut host = Host::new(root.clone());
        let live_pose = persistent_digest(21);
        let live_movement = persistent_digest(22);
        let stored_pose = persistent_digest(23);
        let stored_movement = persistent_digest(24);
        host.quarantine_digest(&live_pose);
        host.quarantine_movement_digest(&live_movement);
        std::fs::write(
            root.join(QUARANTINE_FILE),
            quarantine_text(&[stored_pose.clone()], &[stored_movement.clone()]),
        )
        .expect("write store");

        assert_eq!(
            host.load_persistent_quarantine().expect("load store"),
            2,
            "only the persisted digests are new"
        );
        assert_eq!(
            host.quarantined_digests.iter().cloned().collect::<Vec<_>>(),
            vec![live_pose.clone(), stored_pose],
            "the live entry is kept and the persisted entry appended"
        );
        assert_eq!(
            host.movement_quarantined_digests
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![live_movement.clone(), stored_movement]
        );

        // A persisted copy of an already-live digest is neither counted nor reordered.
        std::fs::write(
            root.join(QUARANTINE_FILE),
            quarantine_text(&[live_pose.clone()], &[live_movement.clone()]),
        )
        .expect("rewrite store");
        assert_eq!(
            host.load_persistent_quarantine().expect("reload store"),
            0
        );
        assert_eq!(
            host.quarantined_digests.len(),
            2,
            "loading never removes a live entry"
        );
        assert_eq!(
            host.quarantined_digests.front(),
            Some(&live_pose),
            "a duplicate does not reorder or duplicate the live entry"
        );
        assert_eq!(host.movement_quarantined_digests.len(), 2);
        assert_eq!(
            host.movement_quarantined_digests.front(),
            Some(&live_movement)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persisted_quarantine_replaces_the_previous_file_and_a_stale_temp() {
        let root = pose_test_root("quarantine-replace");
        let mut host = Host::new(root.clone());
        let first = persistent_digest(31);
        let second = persistent_digest(32);
        host.quarantine_digest(&first);
        host.persist_quarantine().expect("persist first store");
        assert_eq!(
            toml::from_str::<QuarantineFile>(
                &std::fs::read_to_string(root.join(QUARANTINE_FILE)).expect("first store")
            )
            .expect("first store parses")
            .pose,
            vec![first.clone()]
        );

        // An interrupted earlier save leaves a temp file behind; the next save must truncate it
        // rather than fail, then replace the target in one rename.
        std::fs::write(root.join(QUARANTINE_TEMP), b"stale partial store").expect("stale temp");
        host.quarantine_digest(&second);
        host.persist_quarantine().expect("replace store");

        let file = toml::from_str::<QuarantineFile>(
            &std::fs::read_to_string(root.join(QUARANTINE_FILE)).expect("replaced store"),
        )
        .expect("replaced store parses");
        assert_eq!(file.pose, vec![first, second], "both digests, oldest first");
        assert!(
            !root.join(QUARANTINE_TEMP).exists(),
            "the temp file is consumed by the rename"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_quarantine_replacement_keeps_the_previous_store_and_cleans_the_temp() {
        let root = pose_test_root("quarantine-failure");
        let mut host = Host::new(root.clone());
        let first = persistent_digest(41);
        let second = persistent_digest(42);
        host.quarantine_digest(&first);
        host.persist_quarantine().expect("persist first store");
        let previous =
            std::fs::read_to_string(root.join(QUARANTINE_FILE)).expect("first store readable");

        // A temp path that cannot be created fails before the target is touched, so the previous
        // store keeps its exact bytes.
        std::fs::create_dir(root.join(QUARANTINE_TEMP)).expect("block the temp path");
        host.quarantine_digest(&second);
        assert!(
            host.persist_quarantine().is_err(),
            "a blocked temp path must fail the save"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(QUARANTINE_FILE)).expect("previous store readable"),
            previous,
            "the previous store is never truncated before the replacement is written"
        );
        std::fs::remove_dir(root.join(QUARANTINE_TEMP)).expect("unblock the temp path");

        // A target that cannot be replaced fails after the temp is written; the temp is removed and
        // the blocking target is left as it was.
        std::fs::remove_file(root.join(QUARANTINE_FILE)).expect("remove store for failure case");
        std::fs::create_dir(root.join(QUARANTINE_FILE)).expect("block the target path");
        assert!(
            host.persist_quarantine().is_err(),
            "a directory target must fail the replacement"
        );
        assert!(
            root.join(QUARANTINE_FILE).is_dir(),
            "the failed replacement leaves the target alone"
        );
        assert!(
            !root.join(QUARANTINE_TEMP).exists(),
            "the written temp file is cleaned up after a failed rename"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Persistent patch state (`<root>/patch-state.toml`). These cases exercise the document
    // shape, validation, cap/eviction, per-revision seeding and stamp round-trip with in-process
    // probes; no native library is built or loaded. The Bevy plugin/probe fail-open wiring is
    // inspection-only here (this crate has no Bevy App/probe-process harness), and the
    // `SystemTime::checked_add` overflow -> length-only branch is likewise inspection-only because
    // both supported targets represent every `u64`-nanosecond offset from `UNIX_EPOCH`.
    // ------------------------------------------------------------------

    fn patch_state_text(file: &PatchStateFile) -> String {
        toml::to_string(file).expect("serialize patch state fixture")
    }

    fn persisted_revision(
        digest: &str,
        revision: &str,
        total_frame_failures: u32,
        max_eval_micros: u64,
    ) -> PersistedRevisionHealth {
        PersistedRevisionHealth {
            digest: digest.to_owned(),
            revision: revision.to_owned(),
            total_frame_failures,
            max_eval_micros,
        }
    }

    #[test]
    fn persistent_state_round_trips_policy_health_and_stamps() {
        let root = pose_test_root("state-round-trip");
        let mut host = Host::new(root.clone());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 5,
        });
        host.health.total_frame_failures = 3;
        host.health.max_eval_micros = 1234;
        host.sync_persisted_health(ACTIVE_DIGEST, "rev-active");
        let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let pose = ArtifactStamp {
            library: format!("pose-{ACTIVE_DIGEST}.dll"),
            len: 12345,
            modified: Some(modified),
        };
        let movement = ArtifactStamp {
            library: format!("movement-{MOVEMENT_CANARY_DIGEST}.dll"),
            len: 54321,
            modified: None,
        };
        let pose_manifest = format!(
            "revision = \"rev-active\"\nlibrary = \"{}\"\nsha1 = \"{}\"\n",
            pose.library,
            "a".repeat(40)
        );
        let movement_manifest = format!(
            "revision = \"rev-movement\"\nlibrary = \"{}\"\nsha1 = \"{}\"\n",
            movement.library,
            "b".repeat(40)
        );
        host.observed = Some(pose_manifest.clone());
        host.movement_observed = Some(movement_manifest.clone());
        host.set_pose_failure_stamp(Some(pose.clone()));
        host.set_movement_failure_stamp(Some(movement.clone()));

        assert!(host.persist_state().expect("persist patch state"));
        assert!(!host.state_dirty, "a successful save clears the dirty flag");
        assert!(
            !root.join(PATCH_STATE_TEMP).exists(),
            "the temp file is renamed over the target, not left behind"
        );

        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored.load_persistent_state().expect("load patch state"),
            1,
            "one per-revision entry is applied"
        );
        assert_eq!(restored.health_policy(), host.health_policy());
        assert_eq!(restored.persisted_health.len(), 1);
        assert_eq!(restored.persisted_health[0].digest, ACTIVE_DIGEST);
        assert_eq!(restored.persisted_health[0].revision, "rev-active");
        assert_eq!(restored.persisted_health[0].total_frame_failures, 3);
        assert_eq!(restored.persisted_health[0].max_eval_micros, 1234);
        assert_eq!(
            restored.pose_failure_stamp.as_ref(),
            Some(&pose),
            "a first-run artifact stamp round-trips with its modification time"
        );
        assert_eq!(
            restored.movement_failure_stamp.as_ref(),
            Some(&movement),
            "a stamp without a modification time round-trips as length-only"
        );
        assert_eq!(
            restored.observed.as_deref(),
            Some(pose_manifest.as_str()),
            "the latched pose manifest text is restored so the first poll can consult the stamp"
        );
        assert_eq!(
            restored.movement_observed.as_deref(),
            Some(movement_manifest.as_str()),
            "the latched movement manifest text is restored with its stamp"
        );
        assert!(
            !restored.state_dirty,
            "a successful load clears the dirty flag"
        );
        assert!(
            !restored.persist_state().expect("a clean load writes nothing"),
            "an unchanged document is not rewritten"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_missing_file_is_an_empty_success() {
        let root = pose_test_root("state-missing");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_state()
                .expect("a missing state file is not an error"),
            0
        );
        assert!(host.persisted_health.is_empty());
        assert_eq!(host.health_policy(), PatchHealthPolicy::default());
        assert!(host.pose_failure_stamp.is_none());
        assert!(host.movement_failure_stamp.is_none());
        assert!(!host.state_dirty);
        assert!(
            !host.persist_state().expect("a clean state persists as a no-op"),
            "nothing changed, so nothing is written"
        );
        assert!(
            !root.join(PATCH_STATE_FILE).exists(),
            "a fresh install never creates an empty state file"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_rejects_bad_documents_without_touching_memory() {
        let root = pose_test_root("state-bad-documents");
        let mut host = Host::new(root.clone());
        let policy = PatchHealthPolicy {
            max_consecutive_frame_failures: 4,
            max_total_frame_failures: 9,
        };
        host.set_health_policy(policy);
        host.health.total_frame_failures = 2;
        host.health.max_eval_micros = 77;
        host.sync_persisted_health(ACTIVE_DIGEST, "rev-keep");
        let stamp = ArtifactStamp {
            library: format!("pose-{ACTIVE_DIGEST}.dll"),
            len: 1,
            modified: None,
        };
        host.set_pose_failure_stamp(Some(stamp.clone()));
        let baseline_health: Vec<PersistedRevisionHealth> =
            host.persisted_health.iter().cloned().collect();

        for (case, text) in [
            ("malformed", "schema_version = \"one\"\n".to_owned()),
            (
                "unknown-field",
                "schema_version = 1\nextra = true\n".to_owned(),
            ),
            ("unsupported-version", "schema_version = 2\n".to_owned()),
            (
                "missing-version",
                "[policy]\nmax_consecutive_frame_failures = 1\nmax_total_frame_failures = 3\n"
                    .to_owned(),
            ),
            (
                "incomplete-policy",
                "schema_version = 1\n[policy]\nmax_consecutive_frame_failures = 1\n".to_owned(),
            ),
        ] {
            std::fs::write(root.join(PATCH_STATE_FILE), &text).expect("write bad state");
            assert!(
                host.load_persistent_state().is_err(),
                "{case} must be rejected"
            );
            assert_eq!(host.health_policy(), policy, "{case} leaves the policy alone");
            assert_eq!(
                host.persisted_health
                    .iter()
                    .cloned()
                    .collect::<Vec<PersistedRevisionHealth>>(),
                baseline_health,
                "{case} leaves the per-revision store alone"
            );
            assert_eq!(
                host.pose_failure_stamp.as_ref(),
                Some(&stamp),
                "{case} leaves the pose stamp alone"
            );
            assert!(
                host.movement_failure_stamp.is_none(),
                "{case} leaves the movement stamp alone"
            );
            assert!(
                host.state_dirty,
                "{case} does not clear the dirty flag on a failed load"
            );
        }

        std::fs::write(
            root.join(PATCH_STATE_FILE),
            vec![b'a'; (MAX_PATCH_STATE_BYTES + 1) as usize],
        )
        .expect("write oversized state");
        let error = host
            .load_persistent_state()
            .expect_err("an oversized state file is rejected before reading");
        assert!(error.to_string().contains("exceeds"), "{error:#}");
        assert_eq!(host.health_policy(), policy);
        assert_eq!(
            host.persisted_health
                .iter()
                .cloned()
                .collect::<Vec<PersistedRevisionHealth>>(),
            baseline_health
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_skips_invalid_entries_and_dedupes_by_digest() {
        let root = pose_test_root("state-invalid-entries");
        let valid = persistent_digest(0x51);
        let other = persistent_digest(0x52);
        let file = PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: None,
            health: Some(PersistedHealth {
                revisions: vec![
                    persisted_revision("not-a-digest", "rev-invalid-digest", 1, 1),
                    persisted_revision(&other, "", 1, 1),
                    persisted_revision(&other, "rev-control\u{7}", 1, 1),
                    persisted_revision(
                        &other,
                        &"r".repeat(MAX_PERSISTED_REVISION_BYTES + 1),
                        1,
                        1,
                    ),
                    persisted_revision(&valid, "rev-first", 1, 10),
                    persisted_revision(&valid, "rev-updated", 4, 40),
                ],
            }),
            movement_health: None,
            stamps: Some(PersistedStamps {
                pose: Some(PersistedStamp {
                    library: "../escape.dll".to_owned(),
                    len: 1,
                    modified_unix_nanos: None,
                    latch: None,
                }),
                movement: Some(PersistedStamp {
                    library: format!("movement-{other}.dll"),
                    len: MAX_LIBRARY_BYTES + 1,
                    modified_unix_nanos: None,
                    latch: None,
                }),
            }),
        };
        std::fs::write(root.join(PATCH_STATE_FILE), patch_state_text(&file))
            .expect("write state");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_state().expect("load patch state"),
            1,
            "only the one valid deduped revision is applied"
        );
        assert_eq!(host.persisted_health.len(), 1);
        assert_eq!(host.persisted_health[0].digest, valid);
        assert_eq!(
            host.persisted_health[0].revision, "rev-updated",
            "the later duplicate replaces the label and moves to the newest end"
        );
        assert_eq!(
            host.persisted_health[0].total_frame_failures, 4,
            "the later duplicate's counters win"
        );
        assert_eq!(host.persisted_health[0].max_eval_micros, 40);
        assert!(
            host.pose_failure_stamp.is_none(),
            "a path-like basename is skipped rather than trusted"
        );
        assert!(
            host.movement_failure_stamp.is_none(),
            "an oversized artifact length is skipped rather than trusted"
        );
        assert_eq!(
            host.health_policy(),
            PatchHealthPolicy::default(),
            "an absent policy table uses the default 1/3"
        );

        // The persisted document is bounded and carries no path or OS/secret text. The stamp is
        // paired with a valid latch so it is actually written.
        let manifest = format!(
            "revision = \"rev-a\"\nlibrary = \"pose-{valid}.dll\"\nsha1 = \"{}\"\n",
            "e".repeat(40)
        );
        host.observed = Some(manifest);
        host.set_pose_failure_stamp(Some(ArtifactStamp {
            library: format!("pose-{valid}.dll"),
            len: 7,
            modified: Some(std::time::UNIX_EPOCH),
        }));
        assert!(host.persist_state().expect("persist the validated state"));
        let text = std::fs::read_to_string(root.join(PATCH_STATE_FILE))
            .expect("the state file is readable");
        assert!(text.contains(&valid));
        assert!(
            !text.contains('/') && !text.contains(".."),
            "no paths are written: {text}"
        );
        assert!(
            !text.contains("not-a-digest") && !text.contains("escape.dll"),
            "skipped entries are never written back: {text}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_load_respects_the_cap_and_oldest_eviction() {
        let root = pose_test_root("state-cap");
        let revisions: Vec<PersistedRevisionHealth> = (0..20)
            .map(|index| {
                persisted_revision(
                    &persistent_digest(index),
                    &format!("rev-{index}"),
                    index as u32,
                    index as u64,
                )
            })
            .collect();
        let file = PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: None,
            health: Some(PersistedHealth { revisions }),
            movement_health: None,
            stamps: None,
        };
        std::fs::write(root.join(PATCH_STATE_FILE), patch_state_text(&file))
            .expect("write state");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_state().expect("load patch state"),
            MAX_PERSISTED_REVISIONS,
            "an oversized list keeps only its newest cap-sized window"
        );
        assert_eq!(host.persisted_health.len(), MAX_PERSISTED_REVISIONS);
        let oldest = persistent_digest(4);
        let newest = persistent_digest(19);
        assert_eq!(
            host.persisted_health
                .front()
                .map(|entry| entry.digest.as_str()),
            Some(oldest.as_str()),
            "the newest 16 entries start oldest-first at index 4"
        );
        assert_eq!(
            host.persisted_health.back().map(|entry| entry.digest.as_str()),
            Some(newest.as_str())
        );
        assert!(
            !host
                .persisted_health
                .iter()
                .any(|entry| entry.digest == persistent_digest(3)),
            "the oldest entries were evicted at the cap"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persisted_revision_label_is_bounded_to_a_reloadable_value() {
        let root = pose_test_root("state-safe-label");
        let mut host = Host::new(root.clone());
        host.health.total_frame_failures = 2;
        host.health.max_eval_micros = 9;
        host.sync_persisted_health(
            ACTIVE_DIGEST,
            &"r".repeat(MAX_PERSISTED_REVISION_BYTES + 1),
        );
        host.sync_persisted_health(NEXT_DIGEST, "rev-control\u{7}");
        assert_eq!(host.persisted_health[0].revision, "unknown");
        assert_eq!(host.persisted_health[1].revision, "unknown");
        assert!(host.persist_state().expect("persist patch state"));

        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored.load_persistent_state().expect("load patch state"),
            2,
            "both digest-keyed entries survive a reload under a safe label"
        );
        assert_eq!(restored.persisted_health.len(), 2);
        assert!(restored
            .persisted_health
            .iter()
            .all(|entry| entry.revision == "unknown"));
        assert_eq!(restored.persisted_health[0].digest, ACTIVE_DIGEST);
        assert_eq!(restored.persisted_health[0].total_frame_failures, 2);
        assert_eq!(restored.persisted_health[0].max_eval_micros, 9);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_seeds_revision_health_on_candidate_promotion() {
        let root = pose_test_root("state-seed-promotion");
        let file = PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: Some(PersistedPolicy {
                max_consecutive_frame_failures: 0,
                max_total_frame_failures: 4,
            }),
            health: Some(PersistedHealth {
                revisions: vec![persisted_revision(CANARY_DIGEST, "rev-canary", 2, 123)],
            }),
            movement_health: None,
            stamps: None,
        };
        std::fs::write(root.join(PATCH_STATE_FILE), patch_state_text(&file))
            .expect("write state");

        let mut host = Host::new(root.clone());
        host.load_persistent_state().expect("load patch state");
        host.pending = Some(Loaded::probe("rev-canary", CANARY_DIGEST, allow_pose));
        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a valid canary batch promotes")
        );

        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.digest.as_deref(), Some(CANARY_DIGEST));
        assert_eq!(
            snapshot.consecutive_frame_failures, 0,
            "a new activation starts the consecutive streak at 0"
        );
        assert_eq!(
            snapshot.total_frame_failures, 2,
            "the promoted revision is seeded with its persisted cumulative total"
        );
        assert_eq!(
            snapshot.max_eval_micros, 123,
            "the promoted revision is seeded with its persisted high-water mark"
        );
        assert_eq!(
            snapshot.last_eval_micros, 0,
            "the canary activation probe is not timed"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persisted_revision_total_demotes_after_reload() {
        let root = pose_test_root("state-cumulative-demote");
        let policy = PatchHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 2,
        };
        let mut first = Host::new(root.clone());
        first.set_health_policy(policy);
        first.active = Some(Loaded::probe("rev-cumulative", ACTIVE_DIGEST, reject_pose));
        let mut output = Vec::new();
        assert!(
            !first
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a below-threshold active failure is tolerated")
        );
        assert_eq!(first.health_snapshot().total_frame_failures, 1);
        assert_eq!(first.health_snapshot().consecutive_frame_failures, 1);
        assert!(first.persist_state().expect("persist patch state"));

        let mut second = Host::new(root.clone());
        assert_eq!(
            second.load_persistent_state().expect("reload patch state"),
            1
        );
        assert_eq!(second.health_policy(), policy);
        // Reactivate the same verified digest through the real candidate-promotion seed hook. The
        // probe then swaps the in-process function pointer for a failing one because a test cannot
        // build a second native module for the same content identity.
        second.pending = Some(Loaded::probe("rev-cumulative", ACTIVE_DIGEST, allow_pose));
        assert!(
            second
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the reactivated revision promotes")
        );
        assert_eq!(
            second.health_snapshot().total_frame_failures, 1,
            "the reactivation starts from the persisted cumulative total"
        );
        assert_eq!(
            second.health_snapshot().consecutive_frame_failures, 0,
            "the consecutive streak starts at 0 for the new activation"
        );

        second.active = Some(Loaded::probe("rev-cumulative", ACTIVE_DIGEST, reject_pose));
        assert!(
            !second
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the cumulative threshold demotes the module")
        );
        let snapshot = second.health_snapshot();
        assert_eq!(snapshot.total_frame_failures, 2);
        assert_eq!(
            snapshot.consecutive_frame_failures, 1,
            "the consecutive threshold is disabled, so the persisted total caused the demotion"
        );
        assert_eq!(snapshot.revision, None, "no predecessor remained to restore");
        assert!(
            second.is_quarantined(ACTIVE_DIGEST),
            "the demoted revision is quarantined as before"
        );

        // A normal B -> C -> B rewrite is refused by the quarantine gate. After explicit
        // re-admission (cap eviction or deleting the quarantine entry) the stored total seeds the
        // reactivation rather than restarting at zero.
        second.quarantined_digests.clear();
        second.pending = Some(Loaded::probe("rev-cumulative", ACTIVE_DIGEST, allow_pose));
        assert!(
            second
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the re-admitted revision promotes")
        );
        assert_eq!(
            second.health_snapshot().total_frame_failures, 2,
            "the re-admitted revision starts from its stored cumulative total"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_restores_pose_stamp_and_detects_an_artifact_change() {
        let root = pose_test_root("state-stamp");
        let digest = persistent_digest(0x61);
        let library = format!("pose-{digest}.dll");
        std::fs::write(root.join(&library), b"artifact").expect("write artifact");
        let stamp = artifact_stamp(&root, &library).expect("the artifact stats");
        let manifest = format!(
            "revision = \"rev-a\"\nlibrary = \"{library}\"\nsha1 = \"{}\"\n",
            "d".repeat(40)
        );
        let mut host = Host::new(root.clone());
        host.observed = Some(manifest);
        host.set_pose_failure_stamp(Some(stamp.clone()));
        assert!(host.persist_state().expect("persist patch state"));
        assert!(stamp.matches_current(&root));

        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored.load_persistent_state().expect("load patch state"),
            0
        );
        assert_eq!(
            restored.pose_failure_stamp.as_ref(),
            Some(&stamp),
            "the same-text latch identity is restored across a restart"
        );
        assert!(
            !restored.state_dirty,
            "a successful load clears the dirty flag"
        );
        assert!(
            restored
                .pose_failure_stamp
                .as_ref()
                .is_some_and(|stamp| stamp.matches_current(&root)),
            "the restored stamp matches the unchanged artifact"
        );

        std::fs::write(root.join(&library), b"artifact repaired in place")
            .expect("change the artifact length");
        assert!(
            !restored
                .pose_failure_stamp
                .as_ref()
                .is_some_and(|stamp| stamp.matches_current(&root)),
            "a changed artifact length clears the latch and retries the same text once"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_skips_oversized_and_out_of_range_stamps() {
        let root = pose_test_root("state-stamp-bounds");
        let mut host = Host::new(root.clone());
        let oversized = ArtifactStamp {
            library: format!("pose-{ACTIVE_DIGEST}.dll"),
            len: MAX_LIBRARY_BYTES + 1,
            modified: None,
        };
        let future = ArtifactStamp {
            library: format!("movement-{MOVEMENT_CANARY_DIGEST}.dll"),
            len: 7,
            modified: std::time::UNIX_EPOCH
                .checked_add(std::time::Duration::from_nanos(u64::MAX)),
        };
        let movement_manifest = format!(
            "revision = \"rev-m\"\nlibrary = \"{}\"\nsha1 = \"{}\"\n",
            future.library,
            "c".repeat(40)
        );
        host.movement_observed = Some(movement_manifest);
        host.set_pose_failure_stamp(Some(oversized));
        host.set_movement_failure_stamp(Some(future));
        assert!(host.persist_state().expect("persist bounded state"));

        let mut restored = Host::new(root.clone());
        restored.load_persistent_state().expect("reload state");
        assert!(
            restored.pose_failure_stamp.is_none(),
            "an oversized artifact stamp is not persisted"
        );
        assert_eq!(
            restored.movement_failure_stamp,
            Some(ArtifactStamp {
                library: format!("movement-{MOVEMENT_CANARY_DIGEST}.dll"),
                len: 7,
                modified: None,
            }),
            "an mtime that cannot fit TOML's signed integer reloads as length-only"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_none_mtime_compares_by_length_only() {
        let root = pose_test_root("state-stamp-length-only");
        let digest = persistent_digest(0x62);
        let library = format!("pose-{digest}.dll");
        std::fs::write(root.join(&library), b"artifact").expect("write artifact");
        let mut stamp = artifact_stamp(&root, &library).expect("the artifact stats");
        stamp.modified = None;
        assert!(
            stamp.matches_current(&root),
            "a stamp without a modification time compares by length only"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_restored_stamp_drives_the_same_text_retry() {
        let root = pose_test_root("state-stamp-poll");
        let digest = persistent_digest(0x63);
        let library = format!("pose-{digest}.dll");
        std::fs::write(root.join(&library), b"artifact").expect("write artifact");
        let manifest = format!(
            "revision = \"rev-a\"\nlibrary = \"{library}\"\nsha1 = \"{}\"\n",
            "0".repeat(40)
        );
        std::fs::write(root.join("ready.toml"), &manifest).expect("write manifest");
        let stamp = artifact_stamp(&root, &library).expect("the artifact stats");

        let mut first = Host::new(root.clone());
        first.observed = Some(manifest.clone());
        first.set_pose_failure_stamp(Some(stamp));
        assert!(first.persist_state().expect("persist the latch stamp"));

        let mut restored = Host::new(root.clone());
        restored.load_persistent_state().expect("reload state");
        assert_eq!(
            restored.observed.as_deref(),
            Some(manifest.as_str()),
            "the latch text is restored from the state file"
        );
        assert!(
            !restored
                .poll()
                .expect("the restored stamp keeps the unchanged text latched"),
            "the same text with an unchanged artifact stays a quiet latch"
        );
        std::fs::write(root.join(&library), b"artifact repaired in place")
            .expect("change the artifact length");
        assert!(
            restored.poll().is_err(),
            "a changed artifact clears the restored latch and retries"
        );
        assert_eq!(
            restored.take_publication_reason(),
            Some(PatchRejectReason::DigestMismatch),
            "the retry reaches verification again"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_seeds_restored_predecessor_health() {
        let mut host = Host::new(std::env::temp_dir());
        host.persisted_health.push_back(PersistedRevisionHealth {
            digest: NEXT_DIGEST.to_owned(),
            revision: "rev-restored".to_owned(),
            total_frame_failures: 5,
            max_eval_micros: 9,
        });
        host.active = Some(Loaded::probe("rev-active", ACTIVE_DIGEST, reject_pose));
        host.previous
            .push_back(Loaded::probe("rev-restored", NEXT_DIGEST, allow_pose));
        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the valid predecessor restores")
        );
        let snapshot = host.health_snapshot();
        assert_eq!(snapshot.digest.as_deref(), Some(NEXT_DIGEST));
        assert_eq!(snapshot.consecutive_frame_failures, 0);
        assert_eq!(snapshot.total_frame_failures, 5);
        assert_eq!(snapshot.max_eval_micros, 9);
        assert_eq!(
            snapshot.last_eval_micros, 0,
            "the restore probe is not timed"
        );
    }

    #[test]
    fn persistent_state_clamps_a_saturated_high_water_mark_for_toml() {
        let root = pose_test_root("state-saturated-max");
        let mut host = Host::new(root.clone());
        host.health.max_eval_micros = u64::MAX;
        host.sync_persisted_health(ACTIVE_DIGEST, "rev-max");
        assert!(
            host.persist_state().expect("a saturated high-water mark is serializable"),
            "the dirty clamp is written"
        );
        let mut restored = Host::new(root.clone());
        restored.load_persistent_state().expect("reload the clamped state");
        assert_eq!(
            restored.persisted_health[0].max_eval_micros,
            i64::MAX as u64,
            "TOML's signed integer bound is the persisted high-water mark"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_skips_invalid_latches() {
        let root = pose_test_root("state-invalid-latch");
        let digest = persistent_digest(0x65);
        let library = format!("pose-{digest}.dll");
        let file = PatchStateFile {
            schema_version: PATCH_STATE_SCHEMA_VERSION,
            policy: None,
            health: None,
            movement_health: None,
            stamps: Some(PersistedStamps {
                pose: Some(PersistedStamp {
                    library: library.clone(),
                    len: 1,
                    modified_unix_nanos: None,
                    latch: Some("not a publication".to_owned()),
                }),
                movement: Some(PersistedStamp {
                    library: format!("movement-{digest}.dll"),
                    len: 1,
                    modified_unix_nanos: None,
                    latch: Some(format!(
                        "revision = \"rev-m\"\nlibrary = \"movement-{}\"\nsha1 = \"{}\"\n",
                        "0".repeat(40),
                        "0".repeat(40)
                    )),
                }),
            }),
        };
        std::fs::write(root.join(PATCH_STATE_FILE), patch_state_text(&file))
            .expect("write state");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_persistent_state().expect("load state"),
            0,
            "no revision entries are applied"
        );
        assert!(host.pose_failure_stamp.is_none());
        assert!(host.movement_failure_stamp.is_none());
        assert!(host.observed.is_none());
        assert!(host.movement_observed.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_save_keeps_the_newest_bounded_revisions() {
        let root = pose_test_root("state-save-cap");
        let mut host = Host::new(root.clone());
        for index in 0..(MAX_PERSISTED_REVISIONS + 2) {
            host.health.total_frame_failures = index as u32 + 1;
            host.sync_persisted_health(&persistent_digest(index + 1), "rev");
        }
        assert!(host.persist_state().expect("persist bounded state"));
        let mut restored = Host::new(root.clone());
        restored.load_persistent_state().expect("reload state");
        assert_eq!(restored.persisted_health.len(), MAX_PERSISTED_REVISIONS);
        assert_eq!(
            restored
                .persisted_health
                .front()
                .expect("the oldest retained entry")
                .digest,
            persistent_digest(3),
            "the save-side cap evicts the oldest entries"
        );
        assert_eq!(
            restored
                .persisted_health
                .back()
                .expect("the newest retained entry")
                .digest,
            persistent_digest(MAX_PERSISTED_REVISIONS + 2),
            "the newest entry survives"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persistent_state_dirty_tracks_only_persisted_fields() {
        let mut host = Host::new(std::env::temp_dir());
        assert!(!host.state_dirty);
        host.health.record_failure();
        assert!(
            !host.state_dirty,
            "an unsynced live failure is not yet a persisted change"
        );
        host.health.record_success();
        host.health.record_eval(std::time::Duration::from_micros(5));
        assert!(
            !host.state_dirty,
            "consecutive and last-eval changes alone never mark the state dirty"
        );

        host.health.max_eval_micros = u64::MAX;
        host.active = Some(Loaded::probe("rev-same-max", ACTIVE_DIGEST, allow_pose));
        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the active module produces this frame")
        );
        assert!(
            !host.state_dirty,
            "a success below the high-water mark with no new total does not rewrite the state"
        );

        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 10,
        });
        assert!(host.state_dirty, "a changed policy is persisted");
        host.state_dirty = false;
        let unchanged = host.health_policy();
        host.set_health_policy(unchanged);
        assert!(!host.state_dirty, "an unchanged policy is not a change");

        host.set_movement_health_policy(MovementHealthPolicy {
            max_consecutive_frame_failures: 0,
            max_total_frame_failures: 9,
        });
        assert!(
            !host.state_dirty,
            "the movement policy is a live override and does not dirty the state"
        );

        let stamp = ArtifactStamp {
            library: "pose-test.dll".to_owned(),
            len: 1,
            modified: None,
        };
        host.set_pose_failure_stamp(Some(stamp.clone()));
        assert!(host.state_dirty, "setting a stamp is persisted");
        host.state_dirty = false;
        host.set_pose_failure_stamp(Some(stamp));
        assert!(!host.state_dirty, "an identical stamp is not a change");
        host.set_pose_failure_stamp(None);
        assert!(host.state_dirty, "clearing a stamp is persisted");
        host.state_dirty = false;

        host.active = Some(Loaded::probe("rev-fail", NEXT_DIGEST, reject_pose));
        assert!(
            !host
                .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a below-threshold failure is tolerated")
        );
        assert!(
            host.state_dirty,
            "a cumulative failure-total change is persisted even below the threshold"
        );
    }

    #[test]
    fn failed_state_save_keeps_dirty_and_the_previous_state() {
        let root = pose_test_root("state-save-failure");
        let mut host = Host::new(root.clone());
        host.set_health_policy(PatchHealthPolicy {
            max_consecutive_frame_failures: 2,
            max_total_frame_failures: 6,
        });
        assert!(host.persist_state().expect("first save"));
        let previous =
            std::fs::read_to_string(root.join(PATCH_STATE_FILE)).expect("the state is readable");
        assert!(!host.state_dirty);

        // A temp path that cannot be created fails before the target is touched.
        let changed = PatchHealthPolicy {
            max_consecutive_frame_failures: 3,
            max_total_frame_failures: 7,
        };
        host.set_health_policy(changed);
        std::fs::create_dir(root.join(PATCH_STATE_TEMP)).expect("block the temp path");
        assert!(
            host.persist_state().is_err(),
            "a blocked temp path must fail the save"
        );
        assert!(host.state_dirty, "a failed save leaves the state dirty");
        assert_eq!(
            host.health_policy(),
            changed,
            "a failed save never rolls back in-memory state"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(PATCH_STATE_FILE))
                .expect("the previous state is readable"),
            previous,
            "the previous document is never truncated before the replacement"
        );
        std::fs::remove_dir(root.join(PATCH_STATE_TEMP)).expect("unblock the temp path");

        // A target that cannot be replaced fails after the temp is written; the temp is removed and
        // the blocking target is left as it was.
        std::fs::remove_file(root.join(PATCH_STATE_FILE)).expect("remove the target");
        std::fs::create_dir(root.join(PATCH_STATE_FILE)).expect("block the target path");
        assert!(
            host.persist_state().is_err(),
            "a directory target must fail the replacement"
        );
        assert!(host.state_dirty);
        assert!(
            root.join(PATCH_STATE_FILE).is_dir(),
            "the failed replacement leaves the target alone"
        );
        assert!(
            !root.join(PATCH_STATE_TEMP).exists(),
            "the written temp file is cleaned up after a failed rename"
        );
        std::fs::remove_dir(root.join(PATCH_STATE_FILE)).expect("unblock the target path");

        assert!(
            host.persist_state()
                .expect("the save succeeds after the block is removed")
        );
        assert!(!host.state_dirty, "a successful save clears the dirty flag");
        let text = std::fs::read_to_string(root.join(PATCH_STATE_FILE))
            .expect("the saved state is readable");
        assert!(text.contains("max_total_frame_failures = 7"), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Pose crash sentinel (`<root>/active-sentinel.toml`). These cases exercise the
    // write/clear/load lifecycle with in-process hosts for the loading and staged-canary windows;
    // no native library is built or loaded, and the crash-inside-`Library::new`/`process()` window
    // itself is verified by inspection of the write-before-call/clear-after-return ordering.
    // ------------------------------------------------------------------

    #[test]
    fn active_sentinel_dirty_pose_is_quarantined_and_persisted() {
        let root = pose_test_root("sentinel-quarantine");
        let digest = persistent_digest(51);
        let revision = "rev-crashed";
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"{revision}\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write sentinel");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the dirty sentinel"),
            Some(digest.clone()),
            "the crashed pose digest is recovered from the loading marker"
        );
        assert!(
            host.is_quarantined(&digest),
            "the crashed pose digest is quarantined for the next launch"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the marker remains until the recovered quarantine is committed"
        );

        host.persist_quarantine()
            .expect("persist the recovered quarantine");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "persist commits the store and clears the recovered marker"
        );
        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored
                .load_persistent_quarantine()
                .expect("reload the persisted store"),
            1,
            "the recovered digest survives another crash through the persisted store"
        );
        assert!(
            restored.is_quarantined(&digest),
            "the recovered digest is in the persisted pose store"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_sentinel_writer_rejects_an_oversized_serialization() {
        let root = pose_test_root("sentinel-writer-oversized");
        let mut host = Host::new(root.clone());
        let publication = Publication {
            revision: "r".repeat(MAX_ACTIVE_SENTINEL_BYTES as usize + 1),
            library: format!("pose-{}.dll", "0".repeat(40)),
            sha1: "0".repeat(40),
        };
        let error = host
            .write_active_sentinel(
                SentinelKind::Pose,
                &publication.revision,
                &publication.sha1,
                SentinelStage::Loading,
            )
            .expect_err("the writer must not create a marker the loader rejects");
        assert!(
            error.to_string().contains("exceeds"),
            "the writer names the size cap: {error:#}"
        );
        assert!(!root.join(ACTIVE_SENTINEL_FILE).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pending_sentinel_recovery_is_committed_before_a_new_marker() {
        let root = pose_test_root("sentinel-pending");
        let recovered = persistent_digest(71);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{recovered}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write sentinel");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the marker"),
            Some(recovered.clone())
        );
        assert!(host.sentinel_recovery_pending.is_some());

        // A blocked commit must not let a new load overwrite the recovered marker.
        std::fs::create_dir_all(root.join(QUARANTINE_FILE)).expect("block the store path");
        let new_publication = Publication {
            revision: "rev-new".to_owned(),
            library: format!("pose-{}.dll", "1".repeat(40)),
            sha1: persistent_digest(72),
        };
        assert!(host
            .write_active_sentinel(
                SentinelKind::Pose,
                &new_publication.revision,
                &new_publication.sha1,
                SentinelStage::Loading,
            )
            .is_err());
        let text = std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
            .expect("the recovered marker remains");
        assert!(
            text.contains(&recovered),
            "the recovered digest is not overwritten before its commit: {text}"
        );

        std::fs::remove_dir(root.join(QUARANTINE_FILE)).expect("remove the blocker");
        host.write_active_sentinel(
            SentinelKind::Pose,
            &new_publication.revision,
            &new_publication.sha1,
            SentinelStage::Loading,
        )
        .expect("commit the recovery and write the new marker");
        assert!(
            host.is_quarantined(&recovered),
            "the recovered digest is persisted before the new marker"
        );
        let text = std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
            .expect("the new marker exists");
        assert!(text.contains(&new_publication.sha1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_sentinel_marker_survives_a_persist_failure() {
        let root = pose_test_root("sentinel-persist-order");
        let digest = persistent_digest(52);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write sentinel");
        // A directory at the store path makes the rename fail deterministically.
        std::fs::create_dir_all(root.join(QUARANTINE_FILE)).expect("block the store path");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the marker"),
            Some(digest.clone())
        );
        assert!(host.is_quarantined(&digest));
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the marker is still the durable record"
        );
        assert!(
            host.persist_quarantine().is_err(),
            "the directory target blocks replacement"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a persist failure must leave the marker for the next launch"
        );

        std::fs::remove_dir(root.join(QUARANTINE_FILE)).expect("remove the blocker");
        host.persist_quarantine().expect("persist after the blocker");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the successful commit clears the marker without a separate clear call"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn poll_keeps_a_pending_recovery_marker_when_its_commit_fails() {
        let root = pose_test_root("sentinel-poll-pending");
        let recovered = persistent_digest(73);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{recovered}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write sentinel");
        std::fs::create_dir_all(root.join(QUARANTINE_FILE)).expect("block the store path");

        let bytes = b"new non-pe pose artifact";
        let new_digest = format!("{:x}", Sha1::digest(bytes));
        let new_dll = format!("pose-{new_digest}.dll");
        std::fs::write(root.join(&new_dll), bytes).expect("write new artifact");
        std::fs::write(
            root.join("ready.toml"),
            format!("revision = \"rev-new\"\nlibrary = \"{new_dll}\"\nsha1 = \"{new_digest}\"\n"),
        )
        .expect("write ready.toml");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the marker"),
            Some(recovered.clone())
        );
        let error = host
            .poll()
            .expect_err("the new artifact loads only to the post-verification failure");
        assert!(error.to_string().contains("loading presentation module"));
        assert!(
            host.take_sentinel_error().is_some(),
            "the failed pending commit is reported through the sentinel slot"
        );
        let text = std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
            .expect("the pending recovery marker must not be cleared by the failed poll");
        assert!(
            text.contains(&recovered),
            "the recovered digest is still the marker: {text}"
        );
        assert!(
            !text.contains(&new_digest),
            "the new load did not overwrite the pending recovery marker"
        );

        std::fs::remove_dir(root.join(QUARANTINE_FILE)).expect("remove the blocker");
        host.persist_quarantine().expect("commit the recovered store");
        assert!(!root.join(ACTIVE_SENTINEL_FILE).exists());
        assert!(host.is_quarantined(&recovered));
        assert!(host.is_quarantined(&new_digest));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_sentinel_missing_file_is_none() {
        let root = pose_test_root("sentinel-missing");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("a missing sentinel is a clean state"),
            None
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "a clean state quarantines nothing"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "no sentinel error is recorded while clean"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_sentinel_failures_keep_the_file_and_both_stores() {
        let root = pose_test_root("sentinel-invalid");
        let digest = persistent_digest(61);
        let mut host = Host::new(root.clone());

        // Malformed TOML.
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), "revision = \"unterminated\n")
            .expect("write malformed sentinel");
        assert!(
            host.load_active_sentinel().is_err(),
            "a malformed sentinel document is an error"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a failed sentinel load leaves the file for diagnosis"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "both stores stay empty on a malformed sentinel"
        );
        assert!(host.movement_quarantined_digests.is_empty());

        // Unknown field (a future `library`/path-like key must be denied, not guessed at).
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-x\"\nsha1 = \"{digest}\"\nlibrary = \"pose-{digest}.dll\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write unknown-field sentinel");
        assert!(
            host.load_active_sentinel().is_err(),
            "an unknown sentinel field is an error"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "an unknown-field sentinel keeps the file"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "both stores stay empty on an unknown-field sentinel"
        );

        // Invalid digest: not exactly 40 ASCII hex digits.
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!("revision = \"rev-x\"\nsha1 = \"short\"\nkind = \"pose\"\nstage = \"loading\"\n"),
        )
        .expect("write invalid-digest sentinel");
        assert!(
            host.load_active_sentinel().is_err(),
            "an invalid sentinel digest is an error"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "an invalid-digest sentinel keeps the file"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "both stores stay empty on an invalid-digest sentinel"
        );

        // An unknown stage string is not a valid movement window and remains a TOML/serde parse
        // error: the file and both stores are left untouched.
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-x\"\nsha1 = \"{digest}\"\nkind = \"movement\"\nstage = \"clean\"\n"
            ),
        )
        .expect("write unknown-stage sentinel");
        assert!(
            host.load_active_sentinel().is_err(),
            "an unknown movement stage is rejected"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a rejected unknown-stage sentinel keeps the file"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "an unknown movement stage must not quarantine through the pose store"
        );
        assert!(
            host.movement_quarantined_digests.is_empty(),
            "an unknown movement stage must not quarantine through the movement store"
        );

        // An unknown stage string is not a valid pose window and remains a TOML/serde parse
        // error: the file and both stores are left untouched.
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-x\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"clean\"\n"
            ),
        )
        .expect("write unknown-stage sentinel");
        assert!(
            host.load_active_sentinel().is_err(),
            "an unknown sentinel stage is a parse error"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "an unknown-stage sentinel keeps the file"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "an unknown-stage sentinel must not quarantine"
        );
        assert!(host.movement_quarantined_digests.is_empty());

        // An oversized sentinel is rejected before it is read (valid TOML, only the size check
        // can reject it).
        let oversized = format!(
            "revision = \"{}\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n",
            "r".repeat(MAX_ACTIVE_SENTINEL_BYTES as usize)
        );
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), oversized)
            .expect("write oversized sentinel");
        let error = host
            .load_active_sentinel()
            .expect_err("an oversized sentinel is rejected");
        assert!(error.to_string().contains("exceeds"), "{error:#}");
        assert!(host.quarantined_digests.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_sentinel_write_clears_and_missing_clear_is_ok() {
        let root = pose_test_root("sentinel-write");
        let mut host = Host::new(root.clone());
        let publication = Publication {
            revision: "rev-a".to_owned(),
            library: format!("pose-{}.dll", "0".repeat(40)),
            sha1: persistent_digest(71),
        };
        host.write_active_sentinel(
            SentinelKind::Pose,
            &publication.revision,
            &publication.sha1,
            SentinelStage::Loading,
        )
        .expect("write the active sentinel");
        let text = std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
            .expect("read the written sentinel");
        let file: ActiveSentinelFile = toml::from_str(&text).expect("the sentinel parses");
        assert_eq!(file.revision, "rev-a");
        assert_eq!(file.sha1, persistent_digest(71));
        assert_eq!(file.kind, SentinelKind::Pose);
        assert_eq!(file.stage, SentinelStage::Loading);
        assert!(
            !text.contains("library"),
            "the sentinel is digest-only, no library path: {text}"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_TEMP).exists(),
            "the temp sibling is consumed by the rename"
        );

        host.clear_active_sentinel()
            .expect("clear the written sentinel");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "clear removes the sentinel"
        );
        host.clear_active_sentinel()
            .expect("a missing sentinel clear is Ok");
        assert!(
            host.take_sentinel_error().is_none(),
            "no sentinel error is recorded while clearing cleanly"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn regular_pose_load_failure_leaves_no_sentinel_after_poll() {
        let root = pose_test_root("sentinel-poll");
        let bytes = b"definitely-not-a-native-pose-module";
        let digest = format!("{:x}", Sha1::digest(bytes.as_slice()));
        let dll = format!("pose-{digest}.dll");
        std::fs::write(root.join(&dll), bytes).expect("write dll");
        let manifest = format!("revision = \"rev-a\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n");
        std::fs::write(root.join("ready.toml"), &manifest).expect("write ready.toml");

        let mut host = Host::new(root.clone());
        host.poll()
            .expect_err("a bogus module artifact must fail the load");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a regular pose load failure clears the loading sentinel"
        );
        // The bogus bytes passed content verification and failed the native open as a regular
        // error, so nothing was staged and the loading marker was cleared without a sentinel error.
        assert!(
            !host.has_module(),
            "a failed load stages no candidate"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "a cleared sentinel must not leave a sentinel error"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_sentinel_canary_stage_is_quarantined_and_persisted() {
        let root = pose_test_root("sentinel-canary-quarantine");
        let digest = persistent_digest(91);
        let revision = "rev-canary-crashed";
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"{revision}\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"canary\"\n"
            ),
        )
        .expect("write canary sentinel");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the canary sentinel"),
            Some(digest.clone()),
            "the crashed canary digest is recovered from the marker"
        );
        assert!(
            host.is_quarantined(&digest),
            "the crashed canary digest is quarantined for the next launch"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the marker remains until the recovered quarantine is committed"
        );

        host.persist_quarantine()
            .expect("persist the recovered quarantine");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "persist commits the store and clears the recovered marker"
        );
        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored
                .load_persistent_quarantine()
                .expect("reload the persisted store"),
            1,
            "the recovered digest survives another crash through the persisted store"
        );
        assert!(
            restored.is_quarantined(&digest),
            "the recovered digest is in the persisted pose store"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn successful_canary_clears_the_marker() {
        let root = pose_test_root("sentinel-canary-success");
        let mut host = Host::new(root.clone());
        host.pending = Some(Loaded::probe("rev-candidate", CANARY_DIGEST, allow_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a valid canary batch promotes")
        );
        assert_eq!(host.digest(), Some(CANARY_DIGEST));
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a successful canary clears its marker"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "no sentinel error is recorded on the success path"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejected_canary_clears_the_marker_and_quarantines() {
        let root = pose_test_root("sentinel-canary-reject");
        let mut host = Host::new(root.clone());
        host.pending = Some(Loaded::probe("rev-candidate", CANARY_DIGEST, reject_pose));

        let mut output = Vec::new();
        // No active module, so the evaluate returns the existing tolerated outcome (false).
        let result = host
            .evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
            .expect("a rejected canary leaves no active module");
        assert!(!result, "no module produces output after a rejected canary with no active");
        assert!(host.is_quarantined(CANARY_DIGEST));
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a rejected canary clears its marker"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "no sentinel error is recorded on the rejection path"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn canary_marker_exists_during_the_canary_call() {
        let root = pose_test_root("sentinel-canary-observed");
        // A native fault inside the canary call cannot be exercised in-process: unwinding out of
        // an `extern "C"` probe aborts rather than unwinding through `catch_unwind`. Instead the
        // probe records whether the digest-only marker is already on disk when module code runs,
        // and the clear-after-return ordering is asserted after `evaluate` returns. A real fault
        // leaves the marker by the same write-before-call/clear-after-return ordering.
        unsafe extern "C" fn observe_canary_marker(
            frame: *const PoseFrame,
            out: *mut BoneLocal,
            count: u32,
        ) -> i32 {
            let valid = CANARY_MARKER_ROOT
                .lock()
                .expect("canary marker root lock")
                .as_ref()
                .is_some_and(|root| {
                    std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
                        .ok()
                        .and_then(|text| toml::from_str::<ActiveSentinelFile>(&text).ok())
                        .is_some_and(|file| {
                            file.kind == SentinelKind::Pose
                                && file.stage == SentinelStage::Canary
                                && file.revision == "rev-candidate"
                                && file.sha1 == CANARY_DIGEST
                        })
                });
            CANARY_MARKER_SEEN.store(usize::from(valid), Ordering::SeqCst);
            // SAFETY: the host owns the frame and output buffer for this call.
            unsafe { allow_pose(frame, out, count) }
        }

        *CANARY_MARKER_ROOT
            .lock()
            .expect("canary marker root lock") = Some(root.clone());
        CANARY_MARKER_SEEN.store(0, Ordering::SeqCst);
        let mut host = Host::new(root.clone());
        host.pending = Some(Loaded::probe(
            "rev-candidate",
            CANARY_DIGEST,
            observe_canary_marker,
        ));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a valid canary batch promotes")
        );
        assert_eq!(
            CANARY_MARKER_SEEN.load(Ordering::SeqCst),
            1,
            "the canary marker names the pose canary stage, revision and digest before module code runs"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the marker is cleared immediately after the canary call returns"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "no sentinel error is recorded on the observed canary path"
        );
        *CANARY_MARKER_ROOT
            .lock()
            .expect("canary marker root lock") = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn canary_sentinel_write_failure_is_fail_open() {
        let root = pose_test_root("sentinel-canary-write-fail");
        // Put a directory at the sentinel path so the rename fails.
        std::fs::create_dir_all(root.join(ACTIVE_SENTINEL_FILE)).expect("block sentinel path");
        let mut host = Host::new(root.clone());
        host.pending = Some(Loaded::probe("rev-candidate", CANARY_DIGEST, allow_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("a failed sentinel write must not fail the load")
        );
        assert_eq!(host.digest(), Some(CANARY_DIGEST));
        let error = host
            .take_sentinel_error()
            .expect("the failed write is reported through the sentinel slot");
        assert!(
            error.contains("active sentinel write"),
            "the recorded error is the write failure: {error}"
        );
        assert!(
            !error.contains("clear"),
            "no clear is attempted after a failed write: {error}"
        );
        // No clear is attempted: the directory remains at the sentinel path.
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).is_dir(),
            "the directory blocker remains because no clear ran"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pending_recovery_blocks_a_canary_marker_without_losing_the_old_one() {
        let root = pose_test_root("sentinel-canary-pending");
        let recovered = persistent_digest(92);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{recovered}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write sentinel");
        std::fs::create_dir_all(root.join(QUARANTINE_FILE)).expect("block the store path");

        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the marker"),
            Some(recovered.clone())
        );
        host.pending = Some(Loaded::probe("rev-candidate", CANARY_DIGEST, allow_pose));

        let mut output = Vec::new();
        assert!(
            host.evaluate(&[valid_pose_bone()], &[valid_pose_span()], 0.0, &mut output)
                .expect("the canary still runs when the pending recovery cannot commit"),
            "the failed canary marker write is fail-open for evaluation"
        );
        assert_eq!(host.digest(), Some(CANARY_DIGEST));
        let error = host
            .take_sentinel_error()
            .expect("the blocked recovery commit is reported");
        assert!(
            error.contains("commit pending sentinel recovery"),
            "the recorded error names the pending recovery commit: {error}"
        );
        let text = std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
            .expect("the recovered marker must not be replaced while its commit is blocked");
        assert!(
            text.contains(&recovered),
            "the recovered digest is still the marker: {text}"
        );
        assert!(
            !text.contains(CANARY_DIGEST),
            "the new canary marker did not overwrite the pending recovery: {text}"
        );

        std::fs::remove_dir(root.join(QUARANTINE_FILE)).expect("remove the blocker");
        host.persist_quarantine().expect("commit the recovered store");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the successful commit clears the recovered marker"
        );
        assert!(host.is_quarantined(&recovered));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Movement crash sentinel (`<root>/active-sentinel.toml`, `kind = "movement"`). These cases
    // mirror the pose sentinel lifecycle for the movement loading window and the one-shot first
    // staged proposal; an active movement processor's per-frame calls remain unmarked H03.
    // ------------------------------------------------------------------

    #[test]
    fn movement_loading_sentinel_is_quarantined_and_persisted() {
        let root = movement_test_root("movement-sentinel-quarantine");
        let digest = persistent_digest(201);
        let revision = "rev-crashed-movement";
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"{revision}\"\nsha1 = \"{digest}\"\nkind = \"movement\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write movement sentinel");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the movement sentinel"),
            Some(digest.clone()),
            "the crashed movement digest is recovered from the loading marker"
        );
        assert!(
            host.is_movement_quarantined(&digest),
            "the crashed movement digest is quarantined for the next launch"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "a movement marker must never fill the pose quarantine store"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the marker remains until the recovered quarantine is committed"
        );

        host.persist_quarantine()
            .expect("persist the recovered quarantine");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "persist commits the store and clears the recovered marker"
        );
        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored
                .load_persistent_quarantine()
                .expect("reload the persisted store"),
            1,
            "the recovered movement digest survives another crash through the persisted store"
        );
        assert!(
            restored.is_movement_quarantined(&digest),
            "the recovered digest is in the persisted movement store"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_canary_sentinel_is_quarantined_and_persisted() {
        let root = movement_test_root("movement-sentinel-canary");
        let digest = persistent_digest(202);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{digest}\"\nkind = \"movement\"\nstage = \"canary\"\n"
            ),
        )
        .expect("write movement canary sentinel");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel()
                .expect("recover the movement canary marker"),
            Some(digest.clone()),
            "the crashed movement canary digest is recovered"
        );
        assert!(
            host.is_movement_quarantined(&digest),
            "the crashed movement canary digest is quarantined"
        );
        assert!(
            host.quarantined_digests.is_empty(),
            "a movement canary marker must never fill the pose store"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the marker remains until the recovered quarantine is committed"
        );

        host.persist_quarantine()
            .expect("persist the recovered movement canary quarantine");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "persist commits the store and clears the recovered canary marker"
        );
        let mut restored = Host::new(root.clone());
        assert_eq!(
            restored
                .load_persistent_quarantine()
                .expect("reload the persisted store"),
            1,
            "the recovered movement canary digest survives another crash"
        );
        assert!(restored.is_movement_quarantined(&digest));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_unknown_stage_is_rejected_fail_safe() {
        let root = movement_test_root("movement-sentinel-unknown-stage");
        let digest = persistent_digest(206);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-x\"\nsha1 = \"{digest}\"\nkind = \"movement\"\nstage = \"clean\"\n"
            ),
        )
        .expect("write unknown-stage movement sentinel");
        let mut host = Host::new(root.clone());
        assert!(
            host.load_active_sentinel().is_err(),
            "an unknown movement stage remains a deserialize error"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a rejected unknown-stage sentinel keeps the file"
        );
        assert!(host.quarantined_digests.is_empty());
        assert!(host.movement_quarantined_digests.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_loading_sentinel_writes_only_the_digest() {
        let root = movement_test_root("movement-sentinel-write");
        let mut host = Host::new(root.clone());
        let digest = persistent_digest(203);
        host.write_active_sentinel(
            SentinelKind::Movement,
            "rev-a",
            &digest,
            SentinelStage::Loading,
        )
        .expect("write the movement active sentinel");
        let text = std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
            .expect("read the written sentinel");
        let file: ActiveSentinelFile = toml::from_str(&text).expect("the sentinel parses");
        assert_eq!(file.revision, "rev-a");
        assert_eq!(file.sha1, digest);
        assert_eq!(file.kind, SentinelKind::Movement);
        assert_eq!(file.stage, SentinelStage::Loading);
        assert!(
            !text.contains("library"),
            "the sentinel is digest-only, no library path: {text}"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_TEMP).exists(),
            "the temp sibling is consumed by the rename"
        );

        host.clear_active_sentinel()
            .expect("clear the written sentinel");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "clear removes the sentinel"
        );
        host.clear_active_sentinel()
            .expect("a missing sentinel clear is Ok");
        assert!(
            host.take_sentinel_error().is_none(),
            "no sentinel error is recorded while clearing cleanly"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn regular_movement_load_failure_leaves_no_sentinel() {
        let root = movement_test_root("movement-sentinel-poll");
        let bytes = b"definitely-not-a-native-movement-module";
        let digest = format!("{:x}", Sha1::digest(bytes.as_slice()));
        let dll = format!("movement-{digest}.dll");
        std::fs::write(root.join(&dll), bytes).expect("write dll");
        let manifest = movement_publication_text("rev-a", &dll, &digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement.toml");

        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication),
            "a bogus movement artifact must fail the load"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a regular movement load failure clears the loading sentinel"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "a cleared sentinel must not leave a sentinel error"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_sentinel_write_failure_is_fail_open() {
        let root = movement_test_root("movement-sentinel-write-fail");
        // Put a directory at the sentinel path so the rename fails.
        std::fs::create_dir_all(root.join(ACTIVE_SENTINEL_FILE)).expect("block sentinel path");
        let bytes = b"definitely-not-a-native-movement-module";
        let digest = format!("{:x}", Sha1::digest(bytes.as_slice()));
        let dll = format!("movement-{digest}.dll");
        std::fs::write(root.join(&dll), bytes).expect("write dll");
        let manifest = movement_publication_text("rev-a", &dll, &digest);
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement.toml");

        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        assert_eq!(
            host.stage_movement_processor(),
            Err(MovementRefusal::Publication),
            "a failed sentinel write must not skip the movement load"
        );
        let detail = host
            .take_movement_rejection()
            .expect("the movement publication rejection is reported");
        assert!(
            detail.contains("movement publication"),
            "the reported rejection names the movement publication: {detail}"
        );
        let error = host
            .take_sentinel_error()
            .expect("the failed write is reported through the sentinel slot");
        assert!(
            error.contains("active sentinel write"),
            "the recorded error is the write failure: {error}"
        );
        assert!(
            !error.contains("clear"),
            "no clear is attempted after a failed write: {error}"
        );
        // No clear is attempted: the directory remains at the sentinel path.
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).is_dir(),
            "the directory blocker remains because no clear ran"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recovered_movement_digest_is_refused_before_the_artifact_read() {
        let root = movement_test_root("movement-sentinel-recovered");
        let digest = persistent_digest(204);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{digest}\"\nkind = \"movement\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write movement sentinel");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the movement marker"),
            Some(digest.clone())
        );
        assert!(host.is_movement_quarantined(&digest));
        host.persist_quarantine().expect("commit the recovered store");
        assert!(!root.join(ACTIVE_SENTINEL_FILE).exists());

        // Publish a movement manifest naming the recovered digest; the artifact is absent.
        let manifest = movement_publication_text(
            "rev-recovered",
            &format!("movement-{digest}.dll"),
            &digest,
        );
        std::fs::write(root.join(MOVEMENT_MANIFEST), &manifest).expect("write movement.toml");

        let mut host2 = host;
        host2.set_seam_enabled(ModuleKind::Movement, true);
        assert_eq!(
            host2.stage_movement_processor(),
            Err(MovementRefusal::Quarantined),
            "the recovered movement digest is refused before any artifact read"
        );
        assert!(
            host2.is_movement_quarantined(&digest),
            "the movement store still holds the recovered digest"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn staged_movement_proposal_writes_the_canary_marker_during_process() {
        let _canary_guard = MOVEMENT_CANARY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = movement_test_root("movement-canary-observed");
        *MOVEMENT_CANARY_ROOT
            .lock()
            .expect("movement canary root lock") = Some(root.clone());
        MOVEMENT_CANARY_SEEN.store(0, Ordering::SeqCst);
        MOVEMENT_CANARY_CALLS.store(0, Ordering::SeqCst);
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-candidate",
            MOVEMENT_CANARY_DIGEST,
            observe_movement_canary,
        ));

        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("a valid staged movement proposal");
        assert_eq!(source, MovementProposalSource::Staged);
        assert_eq!(
            MOVEMENT_CANARY_SEEN.load(Ordering::SeqCst),
            1,
            "the movement canary marker names the movement stage, revision and digest before process runs"
        );
        assert_eq!(MOVEMENT_CANARY_CALLS.load(Ordering::SeqCst), 1);
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the marker is cleared immediately after the staged process call returns"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "no sentinel error is recorded on the observed canary path"
        );

        // The marker is one-shot: a second staged proposal runs the module but rewrites nothing.
        MOVEMENT_CANARY_SEEN.store(0, Ordering::SeqCst);
        let (_, second_source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the same staged candidate proposes again");
        assert_eq!(second_source, MovementProposalSource::Staged);
        assert_eq!(MOVEMENT_CANARY_CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(
            MOVEMENT_CANARY_SEEN.load(Ordering::SeqCst),
            0,
            "a second staged proposal must not rewrite the one-shot marker"
        );
        *MOVEMENT_CANARY_ROOT
            .lock()
            .expect("movement canary root lock") = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_movement_proposal_does_not_write_a_marker() {
        let _canary_guard = MOVEMENT_CANARY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = movement_test_root("movement-canary-active");
        *MOVEMENT_CANARY_ROOT
            .lock()
            .expect("movement canary root lock") = Some(root.clone());
        MOVEMENT_CANARY_SEEN.store(0, Ordering::SeqCst);
        MOVEMENT_CANARY_CALLS.store(0, Ordering::SeqCst);
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_active = Some(MovementLoaded::probe_with_digest(
            "rev-candidate",
            MOVEMENT_CANARY_DIGEST,
            observe_movement_canary,
        ));

        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("a valid active movement proposal");
        assert_eq!(source, MovementProposalSource::Active);
        assert_eq!(MOVEMENT_CANARY_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(
            MOVEMENT_CANARY_SEEN.load(Ordering::SeqCst),
            0,
            "an active movement processor must not write a canary marker during process"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "an active movement processor must not write a canary marker"
        );
        assert!(host.take_sentinel_error().is_none());
        *MOVEMENT_CANARY_ROOT
            .lock()
            .expect("movement canary root lock") = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejected_staged_movement_proposal_clears_the_canary_marker() {
        let root = movement_test_root("movement-canary-reject");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-candidate",
            MOVEMENT_CANARY_DIGEST,
            reject_frame,
        ));

        let error = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect_err("a non-zero status rejects the staged proposal");
        assert!(matches!(error, MovementRefusal::Status(7)));
        assert!(
            host.is_movement_quarantined(MOVEMENT_CANARY_DIGEST),
            "the rejected staged movement digest is quarantined"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "a regular staged rejection clears the canary marker"
        );
        assert!(host.take_sentinel_error().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn movement_canary_write_failure_is_fail_open() {
        let root = movement_test_root("movement-canary-write-fail");
        // Put a directory at the sentinel path so the rename fails.
        std::fs::create_dir_all(root.join(ACTIVE_SENTINEL_FILE)).expect("block sentinel path");
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-candidate",
            MOVEMENT_CANARY_DIGEST,
            propose_step,
        ));

        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("a failed sentinel write must not skip the movement proposal");
        assert_eq!(source, MovementProposalSource::Staged);
        let error = host
            .take_sentinel_error()
            .expect("the failed write is reported through the sentinel slot");
        assert!(
            error.contains("active sentinel write"),
            "the recorded error is the write failure: {error}"
        );
        assert!(
            !error.contains("clear"),
            "no clear is attempted after a failed write: {error}"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).is_dir(),
            "the directory blocker remains because no clear ran"
        );

        // The one-shot is consumed: a second staged proposal does not retry the write.
        let (_, second_source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the staged candidate still proposes after the failed write");
        assert_eq!(second_source, MovementProposalSource::Staged);
        assert!(
            host.take_sentinel_error().is_none(),
            "the failed write is not retried on the next staged proposal"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unload_resets_the_movement_canary_one_shot() {
        let _canary_guard = MOVEMENT_CANARY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = movement_test_root("movement-canary-reset");
        *MOVEMENT_CANARY_ROOT
            .lock()
            .expect("movement canary root lock") = Some(root.clone());
        MOVEMENT_CANARY_SEEN.store(0, Ordering::SeqCst);
        MOVEMENT_CANARY_CALLS.store(0, Ordering::SeqCst);
        let mut host = Host::new(root.clone());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-candidate",
            MOVEMENT_CANARY_DIGEST,
            observe_movement_canary,
        ));
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the first staged proposal writes the marker");
        assert_eq!(MOVEMENT_CANARY_SEEN.load(Ordering::SeqCst), 1);

        assert!(host.unload_movement_processor());
        assert!(
            !host.movement_canary_attempted,
            "unloading resets the one-shot attempt"
        );
        host.movement_pending = Some(MovementLoaded::probe_with_digest(
            "rev-candidate",
            MOVEMENT_CANARY_DIGEST,
            observe_movement_canary,
        ));
        MOVEMENT_CANARY_SEEN.store(0, Ordering::SeqCst);
        host.movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("a newly staged candidate writes the marker again");
        assert_eq!(
            MOVEMENT_CANARY_SEEN.load(Ordering::SeqCst),
            1,
            "a new staged candidate gets a fresh canary marker"
        );
        *MOVEMENT_CANARY_ROOT
            .lock()
            .expect("movement canary root lock") = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    // Routing assertion in an existing recovery test: a pose marker never fills the
    // movement quarantine store.
    #[test]
    fn pose_marker_never_fills_movement_quarantine() {
        let root = pose_test_root("sentinel-routing");
        let digest = persistent_digest(205);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write pose sentinel");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the pose marker"),
            Some(digest.clone())
        );
        assert!(host.is_quarantined(&digest));
        assert!(
            !host.is_movement_quarantined(&digest),
            "a pose marker must never fill the movement quarantine store"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Clean-shutdown sentinel semantics. `Runtime::drop` calls the host's clean-exit safety net;
    // these cases verify the host half with in-process hosts and no native library. A missing
    // marker is a clean state, a stale marker is removed, and an uncommitted startup recovery is
    // retained because it is the only durable record until `persist_quarantine` commits it.
    // ------------------------------------------------------------------

    #[test]
    fn pose_poll_reports_typed_publication_reasons() {
        // Malformed TOML is deterministic manifest content.
        let root = pose_test_root("reason-malformed");
        std::fs::write(root.join("ready.toml"), "revision = \"unterminated\n")
            .expect("write malformed manifest");
        let mut host = Host::new(root.clone());
        assert!(host.poll().is_err());
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::ManifestMalformed)
        );
        assert_eq!(
            host.take_publication_reason(),
            None,
            "taking the typed reason clears it"
        );
        let _ = std::fs::remove_dir_all(&root);

        // A non-UTF-8 manifest is unreadable content, not transient I/O.
        let root = pose_test_root("reason-unreadable");
        std::fs::write(root.join("ready.toml"), [0xff, 0xfe])
            .expect("write non-UTF-8 manifest");
        let mut host = Host::new(root.clone());
        assert!(host.poll().is_err());
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::ManifestUnreadable)
        );
        let _ = std::fs::remove_dir_all(&root);

        // A not-yet-visible artifact is transient I/O.
        let root = pose_test_root("reason-transient");
        let digest = persistent_digest(401);
        std::fs::write(
            root.join("ready.toml"),
            format!(
                "revision = \"rev-a\"\nlibrary = \"pose-{digest}.dll\"\nsha1 = \"{digest}\"\n"
            ),
        )
        .expect("write missing-artifact manifest");
        let mut host = Host::new(root.clone());
        assert!(host.poll().is_err());
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::TransientIoPending)
        );
        let _ = std::fs::remove_dir_all(&root);

        // A digest already in the bounded quarantine is refused at the gate.
        let root = pose_test_root("reason-quarantined");
        let digest = persistent_digest(402);
        std::fs::write(
            root.join("ready.toml"),
            format!(
                "revision = \"rev-a\"\nlibrary = \"pose-{digest}.dll\"\nsha1 = \"{digest}\"\n"
            ),
        )
        .expect("write quarantined manifest");
        let mut host = Host::new(root.clone());
        host.quarantine_digest(&digest);
        assert!(host.poll().is_err());
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::Quarantined)
        );
        let _ = std::fs::remove_dir_all(&root);

        // A digest/filename mismatch is a terminal artifact rejection; the digest mismatch is
        // reported before the immutable-filename test.
        let root = pose_test_root("reason-artifact");
        let bytes = b"artifact bytes";
        let digest = format!("{:x}", Sha1::digest(bytes));
        let dll = format!("pose-{digest}.dll");
        std::fs::write(root.join(&dll), bytes).expect("write artifact");
        std::fs::write(
            root.join("ready.toml"),
            format!(
                "revision = \"rev-a\"\nlibrary = \"{dll}\"\nsha1 = \"{}\"\n",
                "0".repeat(40)
            ),
        )
        .expect("write mismatched manifest");
        let mut host = Host::new(root.clone());
        let error = host.poll().expect_err("the digest mismatch is terminal");
        assert_eq!(error.to_string(), "module digest differs from publication");
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::DigestMismatch)
        );
        let _ = std::fs::remove_dir_all(&root);

        // The bytes verify, but the native load fails.
        let root = pose_test_root("reason-load-failed");
        let bytes = b"definitely not a portable executable";
        let digest = format!("{:x}", Sha1::digest(bytes));
        let dll = format!("pose-{digest}.dll");
        std::fs::write(root.join(&dll), bytes).expect("write non-PE artifact");
        std::fs::write(
            root.join("ready.toml"),
            format!("revision = \"rev-a\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n"),
        )
        .expect("write load-failing manifest");
        let mut host = Host::new(root.clone());
        let error = host
            .poll()
            .expect_err("non-PE bytes reach the native load");
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::ModuleLoadFailed)
        );
        // The wrapper's `Display` is the old outer context label; anyhow's alternate formatter
        // appends the source chain, so the previous `{error:#}` text is unchanged and not doubled.
        assert_eq!(error.to_string(), "loading presentation module");
        let chained = format!("{error:#}");
        assert!(
            chained.starts_with("loading presentation module: "),
            "the wrapped load source is preserved: {chained}"
        );
        assert!(host.is_quarantined(&digest));
        let _ = std::fs::remove_dir_all(&root);

        // No manifest is a quiet non-rejection.
        let root = pose_test_root("reason-none");
        let mut host = Host::new(root.clone());
        assert!(!host.poll().expect("no manifest is not an error"));
        assert_eq!(host.take_publication_reason(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pose_poll_reports_fine_verification_reasons() {
        let digest = persistent_digest(403);

        // A path-like library fails the basename test before any filesystem access.
        let root = pose_test_root("reason-basename");
        let non_basename = format!("nested/pose-{digest}.dll");
        std::fs::write(
            root.join("ready.toml"),
            format!(
                "revision = \"rev-a\"\nlibrary = \"{non_basename}\"\nsha1 = \"{digest}\"\n"
            ),
        )
        .expect("write non-basename manifest");
        let mut host = Host::new(root.clone());
        let error = host
            .poll()
            .expect_err("the non-basename library is terminal");
        assert_eq!(
            error.to_string(),
            "library must be a basename inside the publication folder"
        );
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::InvalidBasename)
        );
        let _ = std::fs::remove_dir_all(&root);

        // An empty revision is rejected by verification before its metadata check; the poll's
        // pre-verification artifact stamp is a separate step and does not change the reason.
        let root = pose_test_root("reason-empty-revision");
        std::fs::write(
            root.join("ready.toml"),
            format!("revision = \"\"\nlibrary = \"pose-{digest}.dll\"\nsha1 = \"{digest}\"\n"),
        )
        .expect("write empty-revision manifest");
        let mut host = Host::new(root.clone());
        let error = host.poll().expect_err("the empty revision is terminal");
        assert_eq!(error.to_string(), "empty revision");
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::EmptyRevision)
        );
        let _ = std::fs::remove_dir_all(&root);

        // The size cap is checked from metadata before the artifact is read. `set_len` avoids
        // writing 64 MiB of data; the fixture is removed with the test root.
        let root = pose_test_root("reason-too-large");
        let dll = format!("pose-{digest}.dll");
        let artifact = std::fs::File::create(root.join(&dll)).expect("create oversized artifact");
        artifact
            .set_len(MAX_LIBRARY_BYTES + 1)
            .expect("extend oversized artifact");
        drop(artifact);
        std::fs::write(
            root.join("ready.toml"),
            format!("revision = \"rev-a\"\nlibrary = \"{dll}\"\nsha1 = \"{digest}\"\n"),
        )
        .expect("write oversized manifest");
        let mut host = Host::new(root.clone());
        let error = host.poll().expect_err("the oversized artifact is terminal");
        assert_eq!(error.to_string(), "module exceeds 64 MiB limit");
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::ArtifactTooLarge)
        );
        let _ = std::fs::remove_dir_all(&root);

        // The bytes match the declared digest, but the name is not the immutable digest name.
        let root = pose_test_root("reason-immutable-name");
        let bytes = b"fine verification reason fixture";
        let digest = format!("{:x}", Sha1::digest(bytes));
        let wrong_name = format!("pose-{}.dll", "e".repeat(40));
        std::fs::write(root.join(&wrong_name), bytes).expect("write artifact");
        std::fs::write(
            root.join("ready.toml"),
            format!("revision = \"rev-a\"\nlibrary = \"{wrong_name}\"\nsha1 = \"{digest}\"\n"),
        )
        .expect("write wrong-filename manifest");
        let mut host = Host::new(root.clone());
        let error = host
            .poll()
            .expect_err("the wrong immutable filename is terminal");
        assert_eq!(
            error.to_string(),
            "module must use its immutable digest filename"
        );
        assert_eq!(
            host.take_publication_reason(),
            Some(PatchRejectReason::ImmutableFilename)
        );
        // The unchanged terminal text is a quiet staging poll and leaves no new reason.
        assert!(!host.poll().expect("the latched manifest is quiet"));
        assert_eq!(host.take_publication_reason(), None);
        let _ = std::fs::remove_dir_all(&root);

        // A successful `Ok(true)` staging poll requires a loadable native module. The reason is
        // cleared at poll start and assigned only on the terminal error paths, so the successful
        // `None` invariant is inspection-only here; the quiet `Ok(false)` cases above assert the
        // same reset without a native artifact.
        //
        // Native-only branches are inspection-only in this source slice: a missing entry symbol,
        // a null descriptor, an incompatible header and a missing `process` pointer need real
        // export/descriptor fixtures that only a native module build can provide. `Loaded::open`
        // maps them to `MissingEntryPoint`, `AbiMismatch` (null descriptor and header) and
        // `MissingProcessor`; no native load is faked here.
    }

    #[test]
    fn wrapped_rejection_keeps_the_transient_io_chain_and_reason() {
        let error = PatchRejection::wrapped(
            PatchRejectReason::ModuleLoadFailed,
            "loading presentation module",
            std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
        )
        .into_error();
        assert!(
            transient_io(&error),
            "a wrapped permission/sharing failure stays transient"
        );
        assert_eq!(
            publication_reason(&error),
            Some(PatchRejectReason::ModuleLoadFailed)
        );
    }

    #[test]
    fn movement_verify_failure_carries_no_verified_digest_for_a_fine_reason() {
        let root = movement_test_root("fine-verify-reason");
        let digest = persistent_digest(501);
        let publication = Publication {
            revision: "rev-m".to_owned(),
            library: format!("movement-{digest}.dll"),
            sha1: digest.clone(),
        };
        std::fs::write(root.join(&publication.library), b"wrong movement bytes")
            .expect("write mismatched movement artifact");

        let failure = MovementLoaded::verify(&root, &publication)
            .expect_err("the digest mismatch is a verification failure");
        assert_eq!(failure.refusal, MovementRefusal::Publication);
        assert!(!failure.transient, "a digest mismatch is terminal");
        assert!(
            failure.verified_digest.is_none(),
            "verification fails before any digest is proved"
        );
        assert!(
            failure
                .detail
                .starts_with("movement publication rejected: "),
            "the shared detail prefix is preserved: {}",
            failure.detail
        );
        assert!(
            failure.detail.contains("module digest differs from publication"),
            "the shared verification detail is preserved: {}",
            failure.detail
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_clears_a_stale_marker() {
        let root = pose_test_root("sentinel-clean-shutdown-stale");
        let mut host = Host::new(root.clone());
        let digest = persistent_digest(301);
        host.write_active_sentinel(
            SentinelKind::Pose,
            "rev-stale",
            &digest,
            SentinelStage::Loading,
        )
        .expect("write a stale loading marker");
        assert!(root.join(ACTIVE_SENTINEL_FILE).exists());

        assert!(
            host.clear_active_sentinel_on_clean_shutdown()
                .expect("a stale marker clear is a clean state"),
            "a clean-shutdown clear reports that it removed the stale marker"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the clean-shutdown clear removes the stale marker"
        );
        assert!(
            host.take_sentinel_error().is_none(),
            "the clean-shutdown path returns its errors instead of filling the poll slot"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_clears_only_the_matching_owned_marker() {
        let root = pose_test_root("sentinel-clean-shutdown-matching");
        let mut host = Host::new(root.clone());
        let digest = persistent_digest(305);
        host.write_active_sentinel(
            SentinelKind::Pose,
            "rev-owned",
            &digest,
            SentinelStage::Loading,
        )
        .expect("write a marker this host owns");
        // Rewrite the exact owned identity by hand: content equality, not path ownership, is what
        // authorizes the delete.
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-owned\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("rewrite an identical marker identity");

        assert!(
            host.clear_active_sentinel_on_clean_shutdown()
                .expect("clearing the matching owned marker is a clean state"),
            "the marker whose content still matches this host's identity is cleared"
        );
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the matching owned marker is removed"
        );
        assert!(
            host.sentinel_marker_owned.is_none(),
            "a successful clear drops marker ownership"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_keeps_a_foreign_replacement_marker() {
        let root = pose_test_root("sentinel-clean-shutdown-foreign");
        let mut host = Host::new(root.clone());
        let owned = persistent_digest(306);
        host.write_active_sentinel(
            SentinelKind::Pose,
            "rev-owned",
            &owned,
            SentinelStage::Loading,
        )
        .expect("write a marker this host owns");
        // First replace only the revision and keep the digest: the identity includes the revision,
        // so this is already foreign and must be retained.
        let same_digest_text = format!(
            "revision = \"rev-other\"\nsha1 = \"{owned}\"\nkind = \"pose\"\nstage = \"loading\"\n"
        );
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), &same_digest_text)
            .expect("replace the owned revision with a foreign one");
        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("retaining a same-digest foreign revision is not an error"),
            "a marker with the same digest but a different revision is retained"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
                .expect("the same-digest foreign marker survives"),
            same_digest_text,
            "a revision-only replacement is not deleted"
        );
        assert!(
            host.sentinel_marker_owned.is_some(),
            "a retained foreign marker keeps ownership so a later matching retry can still clear"
        );

        // Then replace the digest too. Ownership is bound to content, so this is no longer owned.
        let foreign = persistent_digest(307);
        let foreign_text = format!(
            "revision = \"rev-foreign\"\nsha1 = \"{foreign}\"\nkind = \"pose\"\nstage = \"loading\"\n"
        );
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), &foreign_text)
            .expect("replace the owned marker with a foreign identity");

        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("retaining a foreign replacement is not an error"),
            "a foreign replacement with a different identity is retained"
        );
        let after = std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
            .expect("the foreign marker survives the clean-shutdown path");
        assert_eq!(after, foreign_text, "the foreign bytes are untouched");
        assert!(
            after.contains(&foreign),
            "the foreign digest is still on disk"
        );
        assert!(
            !after.contains(&owned),
            "the replaced owned digest was not written back"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_keeps_an_unparseable_replacement_of_an_owned_marker() {
        let root = pose_test_root("sentinel-clean-shutdown-unparseable-owned");
        let mut host = Host::new(root.clone());
        host.write_active_sentinel(
            SentinelKind::Pose,
            "rev-owned",
            &persistent_digest(311),
            SentinelStage::Loading,
        )
        .expect("write a marker this host owns");
        let malformed = "this is not valid TOML";
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), malformed)
            .expect("replace the owned marker with unparseable bytes");
        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("retaining an unparseable replacement is not an error"),
            "an unparseable replacement of an owned marker is retained"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
                .expect("the unparseable replacement survives"),
            malformed,
            "the malformed replacement bytes are untouched"
        );
        assert!(
            host.sentinel_marker_owned.is_some(),
            "a malformed replacement does not change this host's ownership record"
        );

        // An oversized replacement is never read; it is retained the same way.
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            "x".repeat((MAX_ACTIVE_SENTINEL_BYTES + 1) as usize),
        )
        .expect("replace the owned marker with an oversized file");
        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("retaining an oversized replacement is not an error"),
            "an oversized replacement of an owned marker is retained without being read"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the oversized replacement remains for the next launch/diagnosis"
        );
        assert!(
            host.sentinel_marker_owned.is_some(),
            "an oversized replacement does not change this host's ownership record"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn persist_quarantine_keeps_a_foreign_replacement_marker() {
        let root = pose_test_root("sentinel-persist-foreign");
        let digest = persistent_digest(309);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write the recovered marker");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the marker"),
            Some(digest.clone())
        );
        assert!(host.sentinel_recovery_pending.is_some());
        assert!(host.sentinel_marker_owned.is_some());

        let foreign = persistent_digest(310);
        let foreign_text = format!(
            "revision = \"rev-foreign\"\nsha1 = \"{foreign}\"\nkind = \"pose\"\nstage = \"loading\"\n"
        );
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), &foreign_text)
            .expect("replace the recovered marker with a foreign identity");

        host.persist_quarantine()
            .expect("the recovered digest is durable");
        assert!(
            host.is_quarantined(&digest),
            "the recovered digest is committed to the store"
        );
        assert!(
            host.sentinel_recovery_pending.is_none(),
            "the recovery is committed even when the marker is foreign"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(ACTIVE_SENTINEL_FILE))
                .expect("the foreign marker survives the recovery commit"),
            foreign_text,
            "the foreign bytes are untouched"
        );
        assert!(
            host.sentinel_marker_owned.is_some(),
            "the foreign replacement does not change this host's ownership record"
        );
        assert!(host.take_sentinel_error().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_treats_a_missing_owned_file_as_clean() {
        let root = pose_test_root("sentinel-clean-shutdown-missing-owned");
        let mut host = Host::new(root.clone());
        host.write_active_sentinel(
            SentinelKind::Pose,
            "rev-owned",
            &persistent_digest(308),
            SentinelStage::Loading,
        )
        .expect("write a marker this host owns");
        std::fs::remove_file(root.join(ACTIVE_SENTINEL_FILE))
            .expect("remove the owned marker before the clean-exit path");

        assert!(
            host.clear_active_sentinel_on_clean_shutdown()
                .expect("a missing owned marker is a clean state"),
            "a previously owned marker that is already gone reports a clean shutdown"
        );
        assert!(!root.join(ACTIVE_SENTINEL_FILE).exists());
        assert!(
            host.sentinel_marker_owned.is_none(),
            "a missing owned marker drops the stale ownership record"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_keeps_a_pending_recovery_marker() {
        let root = pose_test_root("sentinel-clean-shutdown-pending");
        let digest = persistent_digest(302);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write the crashed marker");
        let mut host = Host::new(root.clone());
        assert_eq!(
            host.load_active_sentinel().expect("recover the marker"),
            Some(digest.clone())
        );
        assert!(host.sentinel_recovery_pending.is_some());

        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("a pending recovery is not an error"),
            "a pending recovery marker is retained rather than cleared on clean shutdown"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the uncommitted recovery marker survives the clean-exit path"
        );

        host.persist_quarantine()
            .expect("commit the recovered quarantine");
        assert!(
            !root.join(ACTIVE_SENTINEL_FILE).exists(),
            "persist remains the only path that clears the recovered marker"
        );
        assert!(
            host.is_quarantined(&digest),
            "the recovered digest is committed to the pose store"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_without_a_marker_is_ok() {
        let root = pose_test_root("sentinel-clean-shutdown-empty");
        let mut host = Host::new(root.clone());
        assert!(
            host.clear_active_sentinel_on_clean_shutdown()
                .expect("a missing marker is a clean state"),
            "a fresh clean shutdown is a successful clean state"
        );
        assert!(!root.join(ACTIVE_SENTINEL_FILE).exists());
        assert!(
            host.take_sentinel_error().is_none(),
            "a clean shutdown without a marker records no sentinel error"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_keeps_an_unvalidated_marker() {
        let root = pose_test_root("sentinel-clean-shutdown-unvalidated");
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), "this is not valid TOML")
            .expect("write a malformed marker");
        let mut host = Host::new(root.clone());
        assert!(
            host.load_active_sentinel().is_err(),
            "the malformed marker is rejected"
        );
        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("retaining an unvalidated marker is not an error"),
            "a marker this host could not validate is retained on clean shutdown"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the unvalidated marker remains for the next launch/diagnosis"
        );
        assert!(host.sentinel_marker_owned.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_shutdown_keeps_an_unloaded_marker() {
        let root = pose_test_root("sentinel-clean-shutdown-unloaded");
        let digest = persistent_digest(303);
        std::fs::write(
            root.join(ACTIVE_SENTINEL_FILE),
            format!(
                "revision = \"rev-crashed\"\nsha1 = \"{digest}\"\nkind = \"pose\"\nstage = \"loading\"\n"
            ),
        )
        .expect("write a valid marker");
        let mut host = Host::new(root.clone());
        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("retaining an unloaded marker is not an error"),
            "a marker this host never accepted is retained on clean shutdown"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the unloaded previous-run marker remains for recovery"
        );
        assert!(host.sentinel_marker_owned.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_reload_drops_marker_ownership() {
        let root = pose_test_root("sentinel-clean-shutdown-reload");
        let digest = persistent_digest(304);
        let mut host = Host::new(root.clone());
        host.write_active_sentinel(
            SentinelKind::Pose,
            "rev-owned",
            &digest,
            SentinelStage::Loading,
        )
        .expect("write a marker this host owns");
        assert!(host.sentinel_marker_owned.is_some());

        // Replace the owned marker with unvalidated bytes, then fail to re-read it. Ownership must
        // not survive the failed attempt, so the clean-shutdown path retains the bytes.
        std::fs::write(root.join(ACTIVE_SENTINEL_FILE), "this is not valid TOML")
            .expect("replace with an unvalidated marker");
        assert!(
            host.load_active_sentinel().is_err(),
            "the malformed marker is rejected"
        );
        assert!(
            host.sentinel_marker_owned.is_none(),
            "a failed load must not inherit ownership from an earlier marker"
        );
        assert!(
            !host
                .clear_active_sentinel_on_clean_shutdown()
                .expect("retaining an unvalidated marker is not an error"),
            "the unvalidated marker is retained on clean shutdown"
        );
        assert!(
            root.join(ACTIVE_SENTINEL_FILE).exists(),
            "the unvalidated marker remains for the next launch/diagnosis"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
