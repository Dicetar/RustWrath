//! Local trusted modules only. Validation/rollback is not isolation from native memory faults.
use anyhow::{ensure, Context, Result};
use benilla_hotpatch_abi::{BoneLocal, EntryPoint, Header, PoseFrame, ProcessPose, RigSpan, ENTRY};
use libloading::Library;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    collections::VecDeque,
    path::{Component, Path, PathBuf},
};

const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;

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

impl Loaded {
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
        }
    }

    /// Poll only a complete publication. A failed candidate never replaces the active module.
    pub fn poll(&mut self) -> Result<bool> {
        let text = match std::fs::read_to_string(self.root.join("ready.toml")) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if self.observed.as_ref() == Some(&text) {
            return Ok(false);
        }
        self.observed = Some(text.clone());
        self.pending = None;
        let publication: Publication = toml::from_str(&text)?;
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.digest == publication.sha1)
        {
            return Ok(false);
        }
        let pending = Loaded::open(&self.root, publication)?;
        self.pending = Some(pending);
        Ok(true)
    }

    pub fn has_module(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }
    pub fn revision(&self) -> Option<&str> {
        self.active.as_ref().map(|m| m.revision.as_str())
    }
    pub fn take_rejection(&mut self) -> Option<String> {
        self.rejection.take()
    }

    /// Only this host's digest-named artifacts are collected. A publisher uses the same lock;
    /// the active library, three predecessors and the current publication stay pinned.
    pub fn collect_retired(&self) -> Result<usize> {
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
            let mut pinned: std::collections::HashSet<String> = self
                .previous
                .iter()
                .chain(self.active.iter())
                .chain(self.pending.iter())
                .map(|m| m.filename.clone())
                .collect();
            if let Ok(text) = std::fs::read_to_string(root.join("ready.toml")) {
                if let Ok(publication) = toml::from_str::<Publication>(&text) {
                    pinned.insert(publication.library);
                }
            }
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
        if let Some(candidate) = self.pending.take() {
            match candidate.evaluate(bones, rigs, seconds, output) {
                Ok(()) => {
                    if let Some(old) = self.active.replace(candidate) {
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
        match &self.active {
            Some(module) => {
                module.evaluate(bones, rigs, seconds, output)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
