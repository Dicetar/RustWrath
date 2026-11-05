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
/// content digest, then the immutable `<prefix><digest>.dll` name. A digest mismatch must be
/// reported before the filename test so a partial copy keeps its retryable diagnosis.
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

impl Loaded {
    fn digest_bytes(&self) -> Option<ModuleDigest> {
        digest_bytes(&self.digest)
    }

    fn open(root: &Path, publication: Publication) -> Result<Self> {
        let (path, digest) = verify_publication(root, &publication, ModuleKind::Pose)?;
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

/// Typed refusal from the movement seam. Every `Err` leaves no proposal written and drops the
/// movement slot, so the caller hands prediction back to the baseline controller exactly once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MovementRefusal {
    /// No movement module is staged or active.
    NoModule,
    /// Reading, parsing or verifying `movement.toml` failed. Free-form detail is kept in
    /// [`Host::take_movement_rejection`] for logs.
    Publication,
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
        let publication_detail = |error: &dyn std::fmt::Display| MovementLoadFailure {
            refusal: MovementRefusal::Publication,
            detail: format!("movement publication rejected: {error}"),
        };
        let (path, digest) = verify_publication(root, &publication, ModuleKind::Movement)
            .map_err(|error| publication_detail(&error))?;
        // SAFETY: executable local modules are explicitly trusted, exactly as for the pose seam.
        let library = unsafe { Library::new(&path) }.map_err(|error| publication_detail(&error))?;
        let entry =
            unsafe { library.get::<benilla_hotpatch_abi::MovementEntryPoint>(MOVEMENT_ENTRY) }
                .map_err(|error| publication_detail(&error))?;
        let descriptor = unsafe { entry() };
        if descriptor.is_null() {
            return Err(MovementLoadFailure {
                refusal: MovementRefusal::MissingProcessor,
                detail: format!(
                    "movement module {} returned a null descriptor",
                    publication.revision
                ),
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
    /// The movement seam's own manifest latch, independent of the pose publication.
    movement_observed: Option<String>,
    movement_pending: Option<MovementLoaded>,
    movement_active: Option<MovementLoaded>,
    movement_refusal: Option<MovementRefusal>,
    movement_rejection: Option<String>,
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
            movement_observed: None,
            movement_pending: None,
            movement_active: None,
            movement_refusal: None,
            movement_rejection: None,
        }
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
        let publication: Publication = toml::from_str(&text)?;
        if self.active.as_ref().is_some_and(|m| {
            !publication.revision.is_empty()
                && m.digest == publication.sha1
                && m.filename == publication.library
        }) {
            self.observed = Some(text);
            return Ok(false);
        }
        let pending = Loaded::open(&self.root, publication)?;
        self.pending = Some(pending);
        // Failed reads/loads must remain retryable without republishing ready.toml.
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
    /// drops any staged movement candidate. The full loader text is in
    /// [`Self::take_movement_rejection`].
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
                return Err(self.refuse_movement(
                    MovementRefusal::Publication,
                    format!("movement publication: {error}"),
                ))
            }
        };
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
                Ok(true)
            }
            Err(failure) => Err(self.refuse_movement(failure.refusal, failure.detail)),
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
            host.observed.is_none(),
            "failed poll must not latch observed manifest text: {first_err:#}"
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
        assert!(host.observed.is_none());

        let _ = std::fs::remove_dir_all(&root);
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

        // A movement artifact whose bytes do not match its publication is a typed refusal.
        let digest = "0123456789abcdef0123456789abcdef01234567";
        std::fs::write(
            root.join("movement.toml"),
            format!(
                "revision = \"rev-m\"\nlibrary = \"movement-{digest}.dll\"\nsha1 = \"{digest}\"\n"
            ),
        )
        .expect("write movement manifest");
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
}
