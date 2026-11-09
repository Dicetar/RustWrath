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
/// Bounded in-memory quarantine of publication digests that failed deterministically after their
/// bytes were verified (a content/contract error, not transient I/O). The digest is the quarantine
/// key, so a changed manifest text that still names the same bad artifact is refused before any DLL
/// read or `Library::new`. The pose and movement seams each own a store with this cap. This is not
/// the H03 persistent/typed store.
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

struct Loaded {
    revision: String,
    digest: String,
    process: ProcessPose,
    filename: String,
    // This is deliberately owned beside the function pointer and never exposed to callers.
    _library: Library,
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
            _library: library,
        })
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
/// controller must accept the same proposal and only then call [`Host::commit_movement_processor`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementProposalSource {
    Staged,
    Active,
}

/// Typed refusal from the movement seam. A [`Host::movement_proposal`] `Err` leaves no proposal
/// written and drops the movement slot, so the caller hands prediction back to the baseline
/// controller exactly once. A [`Host::stage_movement_processor`] `Err` drops only the staged
/// candidate and latches as its variant describes; it does not evict an already-active processor
/// except for [`MovementRefusal::SeamDisabled`].
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
    /// `MovementLoaded::open` failure. It is refused before any artifact read or `Library::new`,
    /// even when a changed manifest (`B -> C -> B`) names the same bad artifact again.
    Quarantined,
    /// The descriptor magic, artifact prefix or ABI version named another seam or version.
    Kind,
    /// The descriptor's frame/output layout differs from the running host ABI.
    Layout,
    /// The descriptor published no callable processor (`null` descriptor or process pointer).
    MissingProcessor,
    /// The frame handed in was not a usable clock step.
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
        let library = unsafe { Library::new(&path) }.map_err(|error| {
            // `libloading` exposes the underlying `std::io::Error` as `Error::source()` on Windows,
            // so the anyhow chain still sees a publisher's sharing/lock failure (raw 32/33) while a
            // bad image or missing dependency stays terminal. On Unix `DlOpen` carries no io source,
            // so load-time errors are terminal and only `verify_publication`'s reads can be transient.
            let error = anyhow::Error::new(error);
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

    /// A processor handle for the focused tests that cannot build a native library.
    #[cfg(test)]
    fn probe(revision: &str, process: ProcessMovement) -> Self {
        let digest = "0".repeat(40);
        Self {
            revision: revision.to_owned(),
            filename: format!("{}{digest}.dll", ModuleKind::Movement.prefix()),
            digest,
            process,
            _library: None,
        }
    }
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
    active: Option<Loaded>,
    pending: Option<Loaded>,
    previous: VecDeque<Loaded>,
    rejection: Option<String>,
    active_failure: Option<String>,
    restores: VecDeque<RestoreOutcome>,
    /// Bounded in-memory quarantine of pose digests rejected for a deterministic content/contract
    /// failure in `Loaded::open`. The key is `Publication.sha1`, so a later manifest text that
    /// names the same bad artifact (`B -> C -> B`) is refused before `Library::new` without
    /// re-reading the DLL. Not persistent; H03 owns the typed/persistent store.
    quarantined_digests: VecDeque<String>,
    /// The movement seam's own manifest latch, independent of the pose publication.
    movement_observed: Option<String>,
    movement_pending: Option<MovementLoaded>,
    movement_active: Option<MovementLoaded>,
    movement_refusal: Option<MovementRefusal>,
    movement_rejection: Option<String>,
    /// Bounded in-memory quarantine of movement digests rejected by a terminal post-verification
    /// `MovementLoaded::open` failure: the bytes matched `movement.toml` and the immutable
    /// filename. A changed manifest that still names the same bad artifact (`B -> C -> B`) is
    /// refused before any artifact read or `Library::new`. Deliberately separate from the pose
    /// store so the same bytes under a different ABI cannot suppress the other seam; H03 may unify
    /// the stores under a `(ModuleKind, digest)` key. Not persistent; H03 also owns the
    /// typed/persistent store and the canary/frame quarantine.
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
            active: None,
            pending: None,
            previous: VecDeque::new(),
            rejection: None,
            active_failure: None,
            restores: VecDeque::new(),
            quarantined_digests: VecDeque::new(),
            movement_observed: None,
            movement_pending: None,
            movement_active: None,
            movement_refusal: None,
            movement_rejection: None,
            movement_quarantined_digests: VecDeque::new(),
            seams: ModularGameplayRegistry::new(),
        }
    }

    /// Add a pose digest to the bounded in-memory quarantine. Deduplicates so a repeated failure
    /// does not grow the store, and trims the oldest entry at the cap.
    fn quarantine_digest(&mut self, digest: &str) {
        if self.quarantined_digests.iter().any(|d| d == digest) {
            return;
        }
        if self.quarantined_digests.len() == MAX_QUARANTINED_DIGESTS {
            self.quarantined_digests.pop_front();
        }
        self.quarantined_digests.push_back(digest.to_owned());
    }

    /// Whether a pose digest was quarantined after a terminal `Loaded::open` failure.
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
    /// `MovementLoaded::open` failure.
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
                return if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(false)
                } else {
                    Err(error.into())
                };
            }
        };
        if self.observed.as_ref() == Some(&text) {
            return Ok(false);
        }
        // Any changed manifest invalidates the previous observation and any superseded staged
        // candidate so that reverting after a failed replacement re-processes the prior manifest.
        self.pending = None;
        self.observed = None;
        self.rejection = None;
        let publication: Publication = match toml::from_str(&text) {
            Ok(publication) => publication,
            Err(error) => {
                // A malformed manifest is deterministic content. Latch it so the 1 Hz poll does
                // not re-parse and re-report the same bytes; a changed manifest clears the latch
                // above and is retried. H03 adds typed reasons and persistent quarantine.
                self.observed = Some(text);
                return Err(error.into());
            }
        };
        if self.is_quarantined(&publication.sha1) {
            // A digest rejected for a deterministic content/contract failure is refused before the
            // active-module shortcut or any DLL read/`Library::new`, even when a changed manifest
            // (`B -> C -> B`) names it again. Latch this text so the unchanged manifest is a quiet
            // `Ok(false)`; a genuinely new digest is still evaluated.
            let digest = publication.sha1.clone();
            self.observed = Some(text);
            bail!("pose module digest {digest} is quarantined after a terminal load failure");
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
        let (path, digest) = match verify_publication(&self.root, &publication, ModuleKind::Pose) {
            Ok(verified) => verified,
            Err(error) => {
                if transient_io(&error) || publish_in_progress(&self.root) {
                    return Err(error);
                }
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
                self.observed = Some(text);
                return Err(error);
            }
        };
        self.pending = Some(pending);
        // A successful open latches the manifest even though promotion happens on the next batch.
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
                self.movement_observed = None;
                self.movement_refusal = None;
                self.movement_rejection = None;
            } else if !enabled && was_enabled {
                // A disabled seam must not keep running or promote a movement processor. Keep the
                // manifest latch so garbage collection still pins the publication; enabling clears
                // it and lets the next stage call publish the manifest again.
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
    /// only after all output validates. All calls are synchronous; no DLL worker may escape.
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
        // Validate the host frame before touching `pending`, `active`, or `previous` so a
        // malformed caller span or non-finite input bone never drains rollback history.
        if bones.len() > u32::MAX as usize || rigs.len() > u32::MAX as usize {
            bail!("pose batch exceeds u32 ABI counts");
        }
        if !rigs.iter().all(|rig| {
            (rig.first as usize)
                .checked_add(rig.count as usize)
                .is_some_and(|end| end <= bones.len())
        }) {
            bail!("host rig span out of bounds for {} bones", bones.len());
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
                    self.active_failure = Some(format!(
                        "active module {} failed; restoring a previous module: {error:#}",
                        module.revision
                    ));
                    while let Some(previous) = self.previous.pop_back() {
                        // A retained copy of the failed binary would fail identically. Skip it
                        // rather than spend this frame re-evaluating an already-known failure.
                        if failed_digest.is_some() && previous.digest_bytes() == failed_digest {
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
                self.movement_observed = None;
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
        if !self.is_seam_enabled(ModuleKind::Movement) {
            let already_refused = self.movement_observed.as_deref() == Some(text.as_str());
            self.movement_observed = Some(text);
            self.movement_pending = None;
            self.movement_active = None;
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
            return Ok(false);
        }
        // Any changed movement manifest invalidates the previous observation and any superseded
        // candidate, exactly as the pose manifest does for its own slot.
        self.movement_pending = None;
        self.movement_observed = None;
        let publication: Publication = match toml::from_str(&text) {
            Ok(publication) => publication,
            Err(error) => {
                // A malformed movement manifest is deterministic content. Latch it so the 1 Hz
                // stage call does not re-parse and re-report the same bytes; a changed manifest
                // cleared the latch above and is retried. H03 adds typed reasons.
                self.movement_observed = Some(text);
                return Err(self.refuse_movement(
                    MovementRefusal::Publication,
                    format!("movement publication: {error}"),
                ));
            }
        };
        if self.is_movement_quarantined(&publication.sha1) {
            // A digest quarantined after a terminal post-verification failure is refused before
            // the active-module shortcut or any artifact read/`Library::new`, even when a changed
            // manifest (`B -> C -> B`) names it again. Latch this text so the unchanged manifest
            // is a quiet `Ok(false)`; a genuinely new digest is still evaluated.
            let digest = publication.sha1.clone();
            self.movement_observed = Some(text);
            return Err(self.refuse_movement(
                MovementRefusal::Quarantined,
                format!(
                    "movement module digest {digest} is quarantined after a terminal load failure"
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
        match MovementLoaded::open(&self.root, publication) {
            Ok(pending) => {
                self.movement_pending = Some(pending);
                self.movement_observed = Some(text);
                self.movement_refusal = None;
                self.movement_rejection = None;
                Ok(true)
            }
            Err(failure) => {
                if failure.transient {
                    // Leave `movement_observed` unlatched so an incomplete copy or a
                    // publisher-held artifact is retried unchanged; never quarantine it.
                    return Err(self.refuse_movement(failure.refusal, failure.detail));
                }
                // Only a bytes-verified post-verification failure may quarantine. A verification
                // failure carries `verified_digest: None`, so a manifest error cannot poison a
                // legitimate same-digest repair.
                if let Some(digest) = failure.verified_digest.as_deref() {
                    self.quarantine_movement_digest(digest);
                }
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
    /// On `Err` the movement slot is dropped entirely (no retained predecessor), so the caller
    /// hands prediction back to the baseline controller exactly once. Nothing is written to the
    /// caller's pose when the proposal is refused.
    pub fn movement_proposal(
        &mut self,
        frame: &MovementFrame,
        bounds: &MovementBounds,
    ) -> std::result::Result<(MovementProposal, MovementProposalSource), MovementRefusal> {
        // A seam disabled after staging must not keep proposing or accept a commit; the setter
        // normally evicts first, but this gate keeps a direct registry read authoritative.
        if !self.is_seam_enabled(ModuleKind::Movement) {
            self.unload_movement_processor();
            return Err(self.refuse_movement(
                MovementRefusal::SeamDisabled,
                "movement seam is disabled".to_owned(),
            ));
        }
        let Some((process, revision, staged)) = self.movement_process() else {
            return Err(MovementRefusal::NoModule);
        };
        if !frame.dt_seconds.is_finite() || frame.dt_seconds <= 0.0 {
            return Err(self.drop_movement(
                MovementRefusal::FrameInput,
                format!("movement module {revision}: non-finite or non-positive frame step"),
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
            return Err(self.drop_movement(
                MovementRefusal::Status(status),
                format!("movement module {revision} rejected the frame (status {status})"),
            ));
        }
        match validate_movement_output(frame, &output, bounds) {
            Ok(()) => Ok((
                movement_proposal_of(&output),
                if staged {
                    MovementProposalSource::Staged
                } else {
                    MovementProposalSource::Active
                },
            )),
            Err(refusal) => Err(self.drop_movement(
                refusal,
                format!("movement module {revision} output refused ({refusal:?})"),
            )),
        }
    }

    /// Promote the staged processor after the controller accepted its proposal. Returns whether a
    /// promotion happened; only this makes a candidate active.
    pub fn commit_movement_processor(&mut self) -> bool {
        if !self.is_seam_enabled(ModuleKind::Movement) {
            self.unload_movement_processor();
            return false;
        }
        let Some(candidate) = self.movement_pending.take() else {
            return false;
        };
        self.movement_active = Some(candidate);
        self.movement_refusal = None;
        true
    }

    /// Drop the staged or active movement processor and hand prediction back to the baseline
    /// controller. Returns whether anything was held; a second call is a no-op, so the handback
    /// happens exactly once.
    pub fn unload_movement_processor(&mut self) -> bool {
        let staged = self.movement_pending.take().is_some();
        let active = self.movement_active.take().is_some();
        staged | active
    }

    fn movement_process(&self) -> Option<(ProcessMovement, String, bool)> {
        if let Some(pending) = self.movement_pending.as_ref() {
            return Some((pending.process, pending.revision.clone(), true));
        }
        self.movement_active
            .as_ref()
            .map(|active| (active.process, active.revision.clone(), false))
    }

    fn refuse_movement(&mut self, refusal: MovementRefusal, detail: String) -> MovementRefusal {
        self.movement_refusal = Some(refusal);
        self.movement_rejection = Some(detail);
        refusal
    }

    fn drop_movement(&mut self, refusal: MovementRefusal, detail: String) -> MovementRefusal {
        self.movement_pending = None;
        self.movement_active = None;
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
            !host.movement_processor_active(),
            "a proposal is not an activation"
        );
        assert!(host.commit_movement_processor());
        assert!(host.movement_processor_active());
        let (_, source) = host
            .movement_proposal(&movement_frame(), &MOVEMENT_BOUNDS)
            .expect("the active processor proposes");
        assert_eq!(source, MovementProposalSource::Active);
        assert!(!host.commit_movement_processor(), "nothing left to promote");
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

        host.set_seam_enabled(ModuleKind::Movement, false);
        assert!(
            host.is_movement_quarantined(digest),
            "disabling the seam must not clear the quarantine"
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
}
