//! Local trusted modules only. Validation/rollback is not isolation from native memory faults.
use anyhow::{bail, ensure, Context, Result};
use benilla_hotpatch_abi::{BoneLocal, EntryPoint, Header, PoseFrame, ProcessPose, RigSpan, ENTRY};
use libloading::Library;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    collections::VecDeque,
    path::{Component, Path, PathBuf},
};

const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;
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

impl Loaded {
    fn digest_bytes(&self) -> Option<ModuleDigest> {
        digest_bytes(&self.digest)
    }

    fn open(root: &Path, publication: Publication) -> Result<Self> {
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
            publication.library == format!("pose-{digest}.dll"),
            "module must use its immutable digest filename"
        );
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

pub struct Host {
    root: PathBuf,
    observed: Option<String>,
    active: Option<Loaded>,
    pending: Option<Loaded>,
    previous: VecDeque<Loaded>,
    rejection: Option<String>,
    active_failure: Option<String>,
    restores: VecDeque<RestoreOutcome>,
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
    pub fn collect_retired(&self) -> Result<usize> {
        let Some(observed) = self.observed.as_deref() else {
            return Ok(0);
        };
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
            let Ok(text) = std::fs::read_to_string(root.join("ready.toml")) else {
                return Ok(0);
            };
            if text != observed {
                return Ok(0);
            }
            let Ok(publication) = toml::from_str::<Publication>(&text) else {
                return Ok(0);
            };
            let pinned: std::collections::HashSet<String> = self
                .previous
                .iter()
                .chain(self.active.iter())
                .chain(self.pending.iter())
                .map(|m| m.filename.clone())
                .chain(std::iter::once(publication.library))
                .collect();
            let mut removed = 0;
            for entry in std::fs::read_dir(&root)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let managed = name
                    .strip_prefix("pose-")
                    .and_then(|s| s.strip_suffix(".dll"))
                    .is_some_and(|digest| {
                        digest.len() == 40 && digest.bytes().all(|b| b.is_ascii_hexdigit())
                    });
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
        if !rigs.iter().all(|rig| rig.valid(bones.len())) {
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
        };
        let bad_span = RigSpan {
            first_bone: 0,
            bone_count: 2,
        };
        let err = host
            .evaluate(&[valid_bone], &[bad_span], 0.0, &mut output)
            .expect_err("out-of-bounds RigSpan must fail host pre-validation");
        assert!(err.to_string().contains("host rig span out of bounds"));

        let nan_bone = BoneLocal {
            translation: [f32::NAN, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        let good_span = RigSpan {
            first_bone: 0,
            bone_count: 1,
        };
        let err = host
            .evaluate(&[nan_bone], &[good_span], 0.0, &mut output)
            .expect_err("NaN input bone must fail host pre-validation");
        assert!(err
            .to_string()
            .contains("host input bone contains non-finite"));
    }
}
