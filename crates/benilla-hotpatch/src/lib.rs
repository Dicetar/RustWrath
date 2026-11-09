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
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    collections::VecDeque,
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
/// Bounded in-memory quarantine of publication digests that failed after their bytes were verified
/// (a content/contract error or a single-strike evaluation failure, not transient I/O or a frame
/// rejected by host validation). The digest is the quarantine key, so a changed manifest text that
/// still names the same bad artifact is refused before any DLL read or `Library::new`. The same
/// store covers a post-verification load failure and a loaded processor that is rejected by a canary
/// batch or fails while active; retained copies are dropped or skipped. The pose and movement seams
/// each own a store with this cap. This is not the H03 persistent/typed store.
const MAX_QUARANTINED_DIGESTS: usize = 32;

/// Verified SHA-1 content identity of a loaded module, as raw bytes.
pub type ModuleDigest = [u8; 20];

/// Recorder-safe description of one active-module frame failure and its fallback decision.
///
/// Free-form loader errors stay in `take_active_failure`/logs. This enum carries only the
/// verified content identity (when available) and whether a retained predecessor was promoted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// The failed module was replaced by the newest retained predecessor.
    Restored {
        failed_digest: Option<ModuleDigest>,
        /// `None` only when the restored module's identity could not be encoded.
        restored_digest: Option<ModuleDigest>,
    },
    /// The failed module had no retained predecessor; the baseline pose is active again.
    Unavailable { failed_digest: Option<ModuleDigest> },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    revision: String,
    library: String,
    sha1: String,
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
    let library_path = Path::new(library);
    if library_path.components().count() != 1
        || !matches!(
            library_path.components().next(),
            Some(Component::Normal(_))
        )
    {
        return None;
    }
    let metadata = std::fs::metadata(root.join(library_path)).ok()?;
    Some(ArtifactStamp {
        library: library.to_owned(),
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

impl ArtifactStamp {
    /// Whether the named artifact still has the recorded length and modification time. A missing
    /// or unreadable artifact returns `false` so the caller retries instead of masking a repair.
    fn matches_current(&self, root: &Path) -> bool {
        artifact_stamp(root, &self.library)
            .is_some_and(|current| current.len == self.len && current.modified == self.modified)
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

/// Verify a publication's artifact for one seam and return its pinned path and content digest.
///
/// The order is load-bearing and shared by every seam: basename, non-empty revision, size cap,
/// content digest, then the immutable `<prefix><digest>.dll` name. A digest mismatch is reported
/// before the filename test so the diagnostic names the bytes; `Host::poll` treats that
/// deterministic mismatch as terminal, while an unreadable/incomplete artifact keeps its retry.
fn verify_publication(
    root: &Path,
    publication: &Publication,
    kind: ModuleKind,
) -> Result<(PathBuf, String)> {
    let path = Path::new(&publication.library);
    ensure!(
        path.components().count() == 1
            && matches!(path.components().next(), Some(Component::Normal(_))),
        "library must be a basename inside the publication folder"
    );
    ensure!(!publication.revision.is_empty(), "empty revision");
    let path = root.join(path);
    ensure!(
        std::fs::metadata(&path)?.len() <= MAX_LIBRARY_BYTES,
        "module exceeds 64 MiB limit"
    );
    let bytes = std::fs::read(&path)?;
    let digest = format!("{:x}", Sha1::digest(&bytes));
    ensure!(
        digest == publication.sha1,
        "module digest differs from publication"
    );
    ensure!(
        publication.library == format!("{}{digest}.dll", kind.prefix()),
        "module must use its immutable digest filename"
    );
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
        let library = unsafe { Library::new(&path) }.context("loading presentation module")?;
        let entry =
            unsafe { library.get::<EntryPoint>(ENTRY) }.context("missing module entry point")?;
        let api = unsafe { entry() };
        ensure!(!api.is_null(), "null module descriptor");
        let header = unsafe { std::ptr::read(api.cast::<Header>()) };
        ensure!(header.compatible(), "incompatible presentation ABI");
        let process = unsafe { (*api).process }.context("missing pose processor")?;
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
    ) -> Result<()> {
        ensure!(
            bones.len() <= u32::MAX as usize && rigs.len() <= u32::MAX as usize,
            "pose frame too large"
        );
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
        ensure!(
            status == 0,
            "module {} rejected pose frame (status {status})",
            self.revision
        );
        ensure!(
            output
                .iter()
                .zip(bones)
                .all(|(after, before)| after.valid() && after.parent == before.parent),
            "module returned invalid transforms or changed skeleton topology"
        );
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
/// output-validation refusal drops the failing candidate and quarantines its verified digest: a
/// failing staged candidate retains a healthy active processor with a different digest, while a
/// failing active processor clears both slots. [`MovementRefusal::SeamDisabled`] evicts both. A
/// [`Host::stage_movement_processor`] `Err` drops only the staged candidate and latches as its
/// variant describes; it does not evict an already-active processor except for
/// [`MovementRefusal::SeamDisabled`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MovementRefusal {
    /// No movement module is staged or active.
    NoModule,
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
    fn open(
        root: &Path,
        publication: Publication,
    ) -> std::result::Result<Self, MovementLoadFailure> {
        // A verification failure cannot prove the declared digest against the bytes, so it never
        // supplies `verified_digest`; a transient read or a held publisher lock retries unchanged.
        let (path, digest) = verify_publication(root, &publication, ModuleKind::Movement).map_err(
            |error| MovementLoadFailure {
                refusal: MovementRefusal::Publication,
                detail: format!("movement publication rejected: {error:#}"),
                transient: transient_io(&error) || publish_in_progress(root),
                verified_digest: None,
            },
        )?;
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
                verified_digest: Some(digest),
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
    rejection: Option<String>,
    active_failure: Option<String>,
    restores: VecDeque<RestoreOutcome>,
    /// Bounded in-memory quarantine of pose digests rejected after their bytes were verified: a
    /// terminal `Loaded::open` failure, a rejected canary batch, an active frame failure or a
    /// failing restore predecessor. The key is `Publication.sha1`, so a later manifest text that
    /// names the same bad artifact (`B -> C -> B`) is refused before `Library::new` without
    /// re-reading the DLL. A frame rejected by host validation never reaches this store. Not
    /// persistent; H03 owns the typed/persistent store.
    quarantined_digests: VecDeque<String>,
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
    /// Bounded in-memory quarantine of movement digests rejected after their bytes were verified:
    /// a terminal post-verification `MovementLoaded::open` failure, or a module-produced `Status`
    /// or output-validation failure in `movement_proposal`. A changed manifest that still names
    /// the same bad artifact (`B -> C -> B`) is refused before any artifact read or `Library::new`.
    /// Deliberately separate from the pose store so the same bytes under a different ABI cannot
    /// suppress the other seam; H03 may unify the stores under a `(ModuleKind, digest)` key. Not
    /// persistent; H03 also owns the typed/persistent store and the frame counters.
    movement_quarantined_digests: VecDeque<String>,
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
            rejection: None,
            active_failure: None,
            restores: VecDeque::new(),
            quarantined_digests: VecDeque::new(),
            movement_observed: None,
            movement_failure_stamp: None,
            movement_pending: None,
            movement_active: None,
            movement_refusal: None,
            movement_rejection: None,
            movement_proposal_ready: false,
            movement_quarantined_digests: VecDeque::new(),
            seams: ModularGameplayRegistry::new(),
        }
    }

    /// Add a pose digest to the bounded in-memory quarantine. Deduplicates so a repeated failure
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

    /// Add a movement digest to the bounded in-memory quarantine. Same semantics as the pose
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

    /// Poll only a complete publication. A failed candidate never replaces the active module.
    pub fn poll(&mut self) -> Result<bool> {
        let text = match std::fs::read_to_string(self.root.join("ready.toml")) {
            Ok(text) => text,
            Err(error) => {
                // A staged DLL may be invoked later, after a rig becomes available.
                // Only keep it staged while its publication can still be observed.
                self.pending = None;
                self.observed = None;
                self.pose_failure_stamp = None;
                return if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(false)
                } else {
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
                self.pose_failure_stamp = None;
            } else {
                return Ok(false);
            }
        }
        // Any changed manifest invalidates the previous observation and any superseded staged
        // candidate so that reverting after a failed replacement re-processes the prior manifest.
        self.pending = None;
        self.observed = None;
        self.rejection = None;
        self.pose_failure_stamp = None;
        let publication: Publication = match toml::from_str(&text) {
            Ok(publication) => publication,
            Err(error) => {
                // A malformed manifest is deterministic content. Latch it so the 1 Hz poll does
                // not re-parse and re-report the same bytes; a changed manifest clears the latch
                // above and is retried. There is no trustworthy publication, so no artifact stamp
                // is recorded; H03 adds typed reasons and persistent quarantine.
                self.pose_failure_stamp = None;
                self.observed = Some(text);
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
            self.pose_failure_stamp = None;
            self.observed = Some(text);
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
                    return Err(error);
                }
                // Terminal verification failure: record the attempted artifact identity so a later
                // same-text repair with new bytes is retried instead of being masked by the latch.
                self.pose_failure_stamp = attempt_stamp.clone();
                self.observed = Some(text);
                return Err(error);
            }
        };
        // The digest is now verified against the bytes. A terminal post-verification failure is a
        // deterministic artifact problem, so quarantine it before latching the manifest.
        let quarantine_digest = digest.clone();
        let pending = match Loaded::open(path, digest, publication) {
            Ok(pending) => pending,
            Err(error) => {
                if transient_io(&error) || publish_in_progress(&self.root) {
                    // Leave `observed` unlatched so an incomplete copy is retried unchanged.
                    return Err(error);
                }
                self.quarantine_digest(&quarantine_digest);
                self.pose_failure_stamp = attempt_stamp;
                self.observed = Some(text);
                return Err(error);
            }
        };
        self.pending = Some(pending);
        // A successful open latches the manifest even though promotion happens on the next batch.
        self.pose_failure_stamp = None;
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
    pub fn take_rejection(&mut self) -> Option<String> {
        self.rejection.take()
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
                self.movement_observed = None;
                self.movement_failure_stamp = None;
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
        self.active_failure.take()
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
            match candidate.evaluate(bones, rigs, seconds, output) {
                Ok(()) => {
                    if let Some(old) = self.active.replace(candidate) {
                        self.previous.retain(|m| m.digest != old.digest);
                        self.previous.push_back(old);
                    }
                    while self.previous.len() > 3 {
                        self.previous.pop_front();
                    }
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
                    // Continue through the old processor in this same frame, avoiding a one-frame
                    // return to the unpatched pose when a replacement fails validation.
                }
            }
        }
        if let Some(module) = self.active.take() {
            match module.evaluate(bones, rigs, seconds, output) {
                Ok(()) => {
                    self.active = Some(module);
                    return Ok(true);
                }
                Err(error) => {
                    let failed_digest = module.digest_bytes();
                    // The active module was already loaded and verified; under the single-strike
                    // H03 policy an evaluation failure is treated as an artifact failure.
                    // Quarantine it and drop retained copies so neither this frame's restore nor a
                    // later manifest can invoke it again.
                    self.quarantine_digest(&module.digest);
                    self.previous.retain(|m| m.digest != module.digest);
                    self.active_failure = Some(format!(
                        "active module {} failed; restoring a previous module: {error:#}",
                        module.revision
                    ));
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
                                self.active = Some(previous);
                                self.push_restore(RestoreOutcome::Restored {
                                    failed_digest,
                                    restored_digest,
                                });
                                return Ok(true);
                            }
                            Err(prev_error) => {
                                // The predecessor also produced invalid output. Quarantine it and
                                // drop any deeper retained copy, then keep walking newest-first.
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
                    self.push_restore(RestoreOutcome::Unavailable { failed_digest });
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
                self.movement_failure_stamp = None;
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
            self.movement_failure_stamp = None;
            self.movement_pending = None;
            self.movement_active = None;
            self.movement_proposal_ready = false;
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
                self.movement_failure_stamp = None;
            } else {
                return Ok(false);
            }
        }
        // Any changed movement manifest invalidates the previous observation and any superseded
        // candidate, exactly as the pose manifest does for its own slot.
        self.movement_pending = None;
        self.movement_proposal_ready = false;
        self.movement_observed = None;
        self.movement_failure_stamp = None;
        let publication: Publication = match toml::from_str(&text) {
            Ok(publication) => publication,
            Err(error) => {
                // A malformed movement manifest is deterministic content. Latch it so the 1 Hz
                // stage call does not re-parse and re-report the same bytes; a changed manifest
                // cleared the latch above and is retried. There is no trustworthy publication, so
                // no artifact stamp is recorded; H03 adds typed reasons.
                self.movement_failure_stamp = None;
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
            self.movement_failure_stamp = None;
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
        // Capture the attempted artifact before `MovementLoaded::open` reads it, for the same reason
        // as the pose path: a post-read stat could describe a publisher's completed repair and mask
        // the next identical-text retry.
        let attempt_stamp = artifact_stamp(&self.root, &publication.library);
        match MovementLoaded::open(&self.root, publication) {
            Ok(pending) => {
                self.movement_pending = Some(pending);
                // A newly staged candidate has not proposed yet; only its own accepted proposal
                // may arm the commit gate.
                self.movement_proposal_ready = false;
                self.movement_observed = Some(text);
                self.movement_failure_stamp = None;
                self.movement_refusal = None;
                self.movement_rejection = None;
                Ok(true)
            }
            Err(failure) => {
                if failure.transient {
                    // Leave `movement_observed` unlatched so an incomplete copy or a
                    // publisher-held artifact is retried unchanged; never quarantine it.
                    self.movement_failure_stamp = None;
                    return Err(self.refuse_movement(failure.refusal, failure.detail));
                }
                // Only a bytes-verified post-verification failure may quarantine. A verification
                // failure carries `verified_digest: None`, so a manifest error cannot poison a
                // legitimate same-digest repair.
                if let Some(digest) = failure.verified_digest.as_deref() {
                    self.quarantine_movement_digest(digest);
                }
                self.movement_failure_stamp = attempt_stamp;
                self.movement_observed = Some(text);
                Err(self.refuse_movement(failure.refusal, failure.detail))
            }
        }
    }

    /// Whether a movement processor is staged or active.
    pub fn has_movement_processor(&self) -> bool {
        self.movement_pending.is_some() || self.movement_active.is_some()
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
    /// Nothing is written to the caller's proposal when a proposal is refused. A `FrameInput`
    /// refusal hands prediction back for this frame but leaves `movement_pending`/`movement_active`
    /// untouched, so the same processor can serve the next valid frame; `NoModule` is likewise
    /// non-destructive. A module-produced `Status` or output-validation failure quarantines the
    /// selected module's verified digest and drops the failing candidate: a staged candidate clears
    /// `movement_pending` and retains a healthy active processor unless it has the same digest,
    /// while an active failure clears both slots (this seam keeps no retained predecessor).
    /// `SeamDisabled` evicts both. A valid proposal or commit never quarantines.
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
        let status = unsafe { process(frame, &mut output) };
        if status != 0 {
            self.quarantine_movement_digest(&digest);
            return Err(self.reject_movement_candidate(
                staged,
                &digest,
                MovementRefusal::Status(status),
                format!("movement module {revision} rejected the frame (status {status})"),
            ));
        }
        match validate_movement_output(frame, &output, bounds) {
            Ok(()) => {
                // Only a staged candidate can be promoted later; an active processor already owns
                // the slot, so an active proposal never arms the commit gate.
                self.movement_proposal_ready = staged;
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
                self.quarantine_movement_digest(&digest);
                Err(self.reject_movement_candidate(
                    staged,
                    &digest,
                    refusal,
                    format!("movement module {revision} output refused ({refusal:?})"),
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
    /// failing candidate instead.
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
        self.movement_active = Some(candidate);
        self.movement_proposal_ready = false;
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
    /// happens exactly once.
    pub fn unload_movement_processor(&mut self) -> bool {
        let staged = self.movement_pending.take().is_some();
        let active = self.movement_active.take().is_some();
        self.movement_proposal_ready = false;
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

    /// Record a module-produced movement failure after its digest was quarantined, and dispose of
    /// the failing candidate. A staged candidate is dropped while a healthy active processor is
    /// retained, unless it carries the same digest: identical content would fail identically. An
    /// active processor failure clears both slots because this seam keeps no retained predecessor.
    /// Caller [`MovementRefusal::FrameInput`] errors never reach this helper; they leave both slots
    /// untouched so the same processor can serve the next valid frame.
    fn reject_movement_candidate(
        &mut self,
        staged: bool,
        digest: &str,
        refusal: MovementRefusal,
        detail: String,
    ) -> MovementRefusal {
        self.movement_proposal_ready = false;
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

    /// Counts calls through the test-only dispatch probe. Reset by the host-validation test that
    /// uses it; no other test references the counter.
    static POSE_DISPATCHES: AtomicUsize = AtomicUsize::new(0);

    /// Counts calls through the span-tiling probe. Only the tiling test references this counter, so
    /// it cannot race the host-validation probe above.
    static TILING_DISPATCHES: AtomicUsize = AtomicUsize::new(0);

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
        let rejection = host.take_rejection().expect("candidate rejection detail");
        assert!(
            rejection.contains("rev-candidate"),
            "the rejection names the candidate: {rejection}"
        );
        assert!(
            host.take_restore_outcomes().is_empty(),
            "candidate rejection is not an active-frame failure"
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
        let mut host = Host::new(std::env::temp_dir());
        host.set_seam_enabled(ModuleKind::Movement, true);
        host.movement_pending = Some(MovementLoaded::probe("rev-movement", process));
        host
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
        let mut host = Host::new(std::env::temp_dir());
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
        let mut host = Host::new(std::env::temp_dir());
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
}
