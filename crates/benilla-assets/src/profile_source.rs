//! Bevy `AssetSource` readers over a manifest-defined `benilla_profile::Profile`.
//!
//! This is the 3.3.5a counterpart of [`crate::MpqAssetReader`]: archive precedence, disabled
//! toggles and provenance come from the owner's manifest instead of the vanilla auto-discovered
//! chain. It does not write the install and does not discover archives.
//!
//! Revision boundary: the active profile is an `Arc` behind a short read/write lock, so a reader
//! clones the revision it is about to use and reads outside the lock. A staged candidate is opened
//! and validated entirely off to the side ([`ProfileAssetReader::stage_content_revision`]) and only
//! replaces the active revision at one commit ([`ProfileAssetReader::commit_content_revision`]).
//! A reader that already cloned the old `Arc` keeps reading the old, coherent revision until it is
//! done; a failed candidate leaves the active revision exactly as it was and names the missing or
//! invalid input.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use anyhow::{Context, Result};
use benilla_profile::{Origin, Profile, Status};
use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetSourceBuilder, ErasedAssetReader, PathStream, Reader,
    VecReader,
};
use bevy::asset::AssetApp;
use bevy::prelude::*;

use crate::{
    internal_path, LooseFiles, LooseOpen, MPQ_LOOSE_SOURCE, MPQ_SOURCE,
};

/// One class of content a coherent revision must be able to serve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentInputKind {
    Dbc,
    Model,
    Texture,
    Ui,
}

impl ContentInputKind {
    /// A path names its own class by extension: `.dbc` data, `.m2`/`.mdx` models, `.blp` textures,
    /// and UI text (`.lua`, `.xml`, `.toc`) for everything else.
    pub fn of_path(path: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".dbc") {
            Self::Dbc
        } else if lower.ends_with(".m2") || lower.ends_with(".mdx") {
            Self::Model
        } else if lower.ends_with(".blp") {
            Self::Texture
        } else {
            Self::Ui
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dbc => "dbc",
            Self::Model => "model",
            Self::Texture => "texture",
            Self::Ui => "ui",
        }
    }

    /// The cheapest fact that proves the file is the content its class promises. This is a header
    /// check, not a parse: a candidate that fails here would make the reloaded client read garbage.
    fn check(self, bytes: &[u8]) -> std::result::Result<(), String> {
        match self {
            Self::Dbc => {
                if bytes.len() < 20 {
                    return Err(format!(
                        "a DBC needs at least its 20-byte header, got {} bytes",
                        bytes.len()
                    ));
                }
                if &bytes[..4] != b"WDBC" {
                    return Err(format!(
                        "expected the WDBC signature, found {:?}",
                        String::from_utf8_lossy(&bytes[..4])
                    ));
                }
                Ok(())
            }
            Self::Model => {
                if !(bytes.starts_with(b"MD20") || bytes.starts_with(b"MD21")) {
                    return Err("expected an MD20 or MD21 M2 signature".to_owned());
                }
                Ok(())
            }
            Self::Texture => {
                if !(bytes.starts_with(b"BLP1") || bytes.starts_with(b"BLP2")) {
                    return Err("expected a BLP1 or BLP2 signature".to_owned());
                }
                Ok(())
            }
            Self::Ui => {
                if bytes.is_empty() {
                    return Err("the UI file is empty".to_owned());
                }
                std::str::from_utf8(bytes)
                    .map(|_| ())
                    .map_err(|_| "the UI file is not UTF-8 text".to_owned())
            }
        }
    }
}

/// One input a staged revision is asked to keep serving.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentInput {
    pub kind: ContentInputKind,
    pub path: String,
}

impl ContentInput {
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let kind = ContentInputKind::of_path(&path);
        Self { kind, path }
    }
}

/// One required input as the staged revision resolved it.
#[derive(Clone, Debug)]
pub struct ValidatedContentInput {
    pub input: ContentInput,
    /// `None` when the active revision does not serve this input today: the reload then does not
    /// require it of the candidate either. A reload may add content; it may not take away what
    /// the running client already reads.
    pub served: Option<(PathBuf, u64)>,
}

/// One archive of the revision's manifest, in ascending precedence (later entries win).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentArchive {
    pub origin: Origin,
    pub path: PathBuf,
}

/// Why a candidate revision could not be staged or committed. Every variant names the exact input
/// or file, so a refused reload says what is missing rather than failing silently.
#[derive(Debug)]
pub enum ContentRevisionError {
    /// The manifest, an archive, or the chain built from them did not open.
    Open { path: PathBuf, detail: String },
    /// Two manifest entries name the same archive, so precedence would be decided by line order.
    DuplicateArchive { archive: PathBuf },
    /// A required input is not in the candidate at all.
    MissingInput { input: ContentInput },
    /// A required input resolves to a delete marker: unreadable by design.
    TombstonedInput { input: ContentInput, archive: PathBuf },
    /// A required input resolved but cannot be read.
    UnreadableInput { input: ContentInput, detail: String },
    /// A required input resolved but is not the content its class promises.
    InvalidInput { input: ContentInput, detail: String },
    /// The candidate was staged against a revision that has since been replaced.
    Superseded { staged_base: u64, active: u64 },
    /// A reader panicked while holding the revision lock.
    Lock,
}

impl std::fmt::Display for ContentRevisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, detail } => {
                write!(f, "candidate profile did not open ({}): {detail}", path.display())
            }
            Self::DuplicateArchive { archive } => write!(
                f,
                "candidate manifest lists {} more than once; archive precedence would be ambiguous",
                archive.display()
            ),
            Self::MissingInput { input } => write!(
                f,
                "candidate revision does not provide the required {} input {}",
                input.kind.as_str(),
                input.path
            ),
            Self::TombstonedInput { input, archive } => write!(
                f,
                "candidate revision tombstones the required {} input {} in {}",
                input.kind.as_str(),
                input.path,
                archive.display()
            ),
            Self::UnreadableInput { input, detail } => write!(
                f,
                "candidate revision cannot read the required {} input {}: {detail}",
                input.kind.as_str(),
                input.path
            ),
            Self::InvalidInput { input, detail } => write!(
                f,
                "candidate revision serves an invalid {} input {}: {detail}",
                input.kind.as_str(),
                input.path
            ),
            Self::Superseded {
                staged_base,
                active,
            } => write!(
                f,
                "the candidate was staged on revision {staged_base} but revision {active} is active; \
                 stage again from the current manifest"
            ),
            Self::Lock => write!(f, "the profile revision lock is poisoned"),
        }
    }
}

impl std::error::Error for ContentRevisionError {}

/// A candidate revision: opened, validated, and ready to replace the active one. Holding it does
/// not change what any reader sees.
pub struct StagedContentRevision {
    base: u64,
    serial: u64,
    manifest_path: PathBuf,
    profile_name: String,
    profile: Profile,
    chain: benilla_formats::Chain,
    archives: Vec<ContentArchive>,
    inputs: Vec<ValidatedContentInput>,
}

impl StagedContentRevision {
    /// The revision this candidate was opened against. A commit from any other base is refused.
    pub fn base_serial(&self) -> u64 {
        self.base
    }

    /// The serial the candidate takes on commit.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    /// The candidate's archives in ascending precedence, with their declared origin.
    pub fn archives(&self) -> &[ContentArchive] {
        &self.archives
    }

    pub fn inputs(&self) -> &[ValidatedContentInput] {
        &self.inputs
    }

    /// A one-line provenance record for logs: the manifest, the revision serial and the archives
    /// in precedence order.
    pub fn provenance(&self) -> String {
        format!(
            "{} revision {} from {} [{}]",
            self.profile_name,
            self.serial,
            self.manifest_path.display(),
            describe_archives(&self.archives)
        )
    }
}

/// A revision that has replaced the active one. The chain is the same archive set the profile now
/// resolves, so a caller that owns the synchronous DBC/cache reader swaps both at one point.
pub struct CommittedContentRevision {
    pub serial: u64,
    pub chain: benilla_formats::Chain,
    pub archives: Vec<ContentArchive>,
    pub inputs: Vec<ValidatedContentInput>,
}

impl CommittedContentRevision {
    pub fn provenance(&self) -> String {
        format!("revision {} [{}]", self.serial, describe_archives(&self.archives))
    }
}

fn describe_archives(archives: &[ContentArchive]) -> String {
    archives
        .iter()
        .map(|archive| {
            let origin = match archive.origin {
                Origin::Stock => "stock",
                Origin::Wm => "wm",
            };
            format!("{origin} {}", archive.path.display())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The active revision, swapped as a whole. A reader clones the `Arc` it needs and reads outside
/// the lock, so a swapped-out revision stays readable until the last reader drops it.
struct ActiveRevision {
    profile: Arc<Profile>,
    serial: u64,
}

#[derive(Clone)]
pub struct ProfileAssetReader {
    active: Arc<RwLock<ActiveRevision>>,
    manifest_path: PathBuf,
    data_dir: PathBuf,
    profile_name: String,
    next_serial: Arc<AtomicU64>,
}

/// Registered readers by Bevy source name. An M2 loader needs a sibling `.skin` while it already
/// holds the `.m2` bytes; Bevy's `LoadContext` exposes no second read, so the profile registered on
/// the app is also reachable synchronously here. One process, one route profile.
static PROFILE_READERS: OnceLock<Mutex<HashMap<String, ProfileAssetReader>>> = OnceLock::new();

/// Read `path` synchronously from the profile registered as `source`.
pub(crate) fn read_profile_sync(source: &str, path: &Path) -> Option<Vec<u8>> {
    let reader = ProfileAssetReader::registered(source)?;
    reader.read_bytes(path).ok()
}

impl ProfileAssetReader {
    pub fn open(data_dir: &Path, manifest_path: &Path, profile: &str) -> Result<Self> {
        let opened = Profile::open(data_dir, manifest_path, profile)
            .with_context(|| format!("opening profile `{profile}` from {}", manifest_path.display()))?;
        Ok(Self {
            active: Arc::new(RwLock::new(ActiveRevision {
                profile: Arc::new(opened),
                serial: 1,
            })),
            manifest_path: manifest_path.to_path_buf(),
            data_dir: data_dir.to_path_buf(),
            profile_name: profile.to_owned(),
            next_serial: Arc::new(AtomicU64::new(2)),
        })
    }

    /// The reader registered for Bevy source `source`; `None` when that source was never
    /// registered. This is how the in-process reload path reaches the same revision boundary the
    /// asset source reads through.
    pub fn registered(source: &str) -> Option<Self> {
        let readers = PROFILE_READERS.get()?.lock().ok()?;
        readers.get(source).cloned()
    }

    /// The serial of the revision readers currently see.
    pub fn revision_serial(&self) -> u64 {
        self.active
            .read()
            .map_or(0, |active| active.serial)
    }

    /// Open and validate a candidate revision from the manifest on disk, without changing what any
    /// reader sees.
    ///
    /// `required` names the inputs the revision must keep serving (paths, classed by extension).
    /// A required input is only enforced when the active revision serves it today, so a reload
    /// cannot silently take away content the running client reads, and a profile that was already
    /// missing an input is not refused for it again. Every failure leaves the active revision —
    /// and every reader holding it — exactly as it was.
    ///
    /// If staging is not needed, [`Self::reload`] is the shorthand: stage, then commit.
    pub fn stage_content_revision(
        &self,
        required: &[&str],
    ) -> std::result::Result<StagedContentRevision, ContentRevisionError> {
        let (base, active) = {
            let active = self.active.read().map_err(|_| ContentRevisionError::Lock)?;
            (active.serial, Arc::clone(&active.profile))
        };
        // The candidate is opened under no lock at all: opening archives can take as long as the
        // disk wants, and every reader keeps reading the old revision meanwhile.
        let candidate = Profile::open(&self.data_dir, &self.manifest_path, &self.profile_name)
            .map_err(|error| ContentRevisionError::Open {
                path: self.manifest_path.clone(),
                detail: format!("{error:#}"),
            })?;
        let archives = archive_precedence(&candidate, &self.manifest_path)?;
        let chain = benilla_formats::Chain::from_archives(
            candidate
                .archives()
                .map(|(_, archive)| archive.clone())
                .collect(),
            benilla_formats::Locale::default(),
        )
        .map_err(|error| ContentRevisionError::Open {
            path: self.manifest_path.clone(),
            detail: format!("{error:#}"),
        })?;
        let mut inputs = Vec::with_capacity(required.len());
        for path in required {
            inputs.push(validate_input(&active, &candidate, *path)?);
        }
        Ok(StagedContentRevision {
            base,
            serial: self.next_serial.fetch_add(1, Ordering::Relaxed),
            manifest_path: self.manifest_path.clone(),
            profile_name: self.profile_name.clone(),
            profile: candidate,
            chain,
            archives,
            inputs,
        })
    }

    /// Replace the active revision with a staged candidate at one point.
    ///
    /// The commit is refused — leaving the active revision untouched — when the candidate was
    /// staged against a revision that has since been replaced, so two reloads can never interleave
    /// into a mixed archive set. Readers that already cloned the old revision finish on it.
    pub fn commit_content_revision(
        &self,
        staged: StagedContentRevision,
    ) -> std::result::Result<CommittedContentRevision, ContentRevisionError> {
        if staged.manifest_path != self.manifest_path {
            return Err(ContentRevisionError::Superseded {
                staged_base: staged.base,
                active: self.revision_serial(),
            });
        }
        let StagedContentRevision {
            base,
            serial,
            profile,
            chain,
            archives,
            inputs,
            manifest_path: _,
            profile_name: _,
        } = staged;
        let mut active = self.active.write().map_err(|_| ContentRevisionError::Lock)?;
        check_commit_base(base, active.serial)?;
        *active = ActiveRevision {
            profile: Arc::new(profile),
            serial,
        };
        Ok(CommittedContentRevision {
            serial,
            chain,
            archives,
            inputs,
        })
    }

    /// Candidate-first reload: a failed reopen leaves the existing profile active.
    pub fn reload(&self) -> Result<()> {
        let staged = self
            .stage_content_revision(&[])
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        self.commit_content_revision(staged)
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub fn status(&self, name: &str) -> Result<Status> {
        Ok(self.snapshot()?.status(name))
    }

    fn snapshot(&self) -> std::result::Result<Arc<Profile>, AssetReaderError> {
        self.active
            .read()
            .map(|active| Arc::clone(&active.profile))
            .map_err(|_| {
                AssetReaderError::Io(Arc::new(std::io::Error::other(
                    "profile lock poisoned",
                )))
            })
    }

    fn read_bytes(&self, path: &Path) -> std::result::Result<Vec<u8>, AssetReaderError> {
        let internal = internal_path(path)?;
        // The revision is cloned under the lock and read outside it: an open candidate or a
        // commit never blocks a read, and this read never sees a half-swapped archive set.
        self.snapshot()?
            .read(&internal)
            .map_err(|error| {
                AssetReaderError::Io(Arc::new(std::io::Error::other(format!("{error:#}"))))
            })
    }
}

/// The candidate's archives in manifest precedence, refusing a manifest that names one twice.
fn archive_precedence(
    profile: &Profile,
    manifest_path: &Path,
) -> std::result::Result<Vec<ContentArchive>, ContentRevisionError> {
    let ordered: Vec<(Origin, PathBuf)> = profile
        .archives()
        .map(|(origin, archive)| (origin, archive.path().to_path_buf()))
        .collect();
    if ordered.is_empty() {
        return Err(ContentRevisionError::Open {
            path: manifest_path.to_path_buf(),
            detail: "the profile resolves no archives".to_owned(),
        });
    }
    if let Some(archive) = first_duplicate_archive(ordered.iter().map(|(_, path)| path.clone())) {
        return Err(ContentRevisionError::DuplicateArchive { archive });
    }
    Ok(ordered
        .into_iter()
        .map(|(origin, path)| ContentArchive { origin, path })
        .collect())
}

/// The first archive path a manifest names more than once, if any. Two entries for one file make
/// precedence a matter of line order rather than of distinct patch layers, so a candidate with one
/// cannot be called coherent.
fn first_duplicate_archive(paths: impl Iterator<Item = PathBuf>) -> Option<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    for path in paths {
        if seen.contains(&path) {
            return Some(path);
        }
        seen.push(path);
    }
    None
}

/// The commit guard: a candidate staged on one revision may only replace that revision. Two
/// reloads cannot interleave into a mixed archive set.
fn check_commit_base(
    staged_base: u64,
    active: u64,
) -> std::result::Result<(), ContentRevisionError> {
    if staged_base != active {
        return Err(ContentRevisionError::Superseded {
            staged_base,
            active,
        });
    }
    Ok(())
}

/// Validate one required input against the candidate, enforcing the no-regression rule against the
/// active revision.
fn validate_input(
    active: &Profile,
    candidate: &Profile,
    path: &str,
) -> std::result::Result<ValidatedContentInput, ContentRevisionError> {
    let input = ContentInput::new(path);
    let served_today = !matches!(active.status(path), Status::Missing);
    validate_input_from(input, served_today, candidate.status(path), || {
        candidate
            .read(path)
            .map_err(|error| format!("{error:#}"))
    })
}

/// The decision half of [`validate_input`], without a `Profile`: the status the active revision
/// reports, the status the candidate reports and the candidate's reader.
fn validate_input_from(
    input: ContentInput,
    served_today: bool,
    candidate: Status,
    read: impl FnOnce() -> std::result::Result<Vec<u8>, String>,
) -> std::result::Result<ValidatedContentInput, ContentRevisionError> {
    if !served_today {
        // The running revision cannot serve this input either, so requiring it of the candidate
        // would refuse every reload on a profile that never had it.
        return Ok(ValidatedContentInput {
            input,
            served: None,
        });
    }
    match candidate {
        Status::Found { archive, size } => {
            let bytes = read().map_err(|detail| ContentRevisionError::UnreadableInput {
                input: input.clone(),
                detail,
            })?;
            input.kind.check(&bytes).map_err(|detail| {
                ContentRevisionError::InvalidInput {
                    input: input.clone(),
                    detail,
                }
            })?;
            Ok(ValidatedContentInput {
                input,
                served: Some((archive, size)),
            })
        }
        Status::Tombstoned { archive } => Err(ContentRevisionError::TombstonedInput { input, archive }),
        Status::Missing => Err(ContentRevisionError::MissingInput { input }),
    }
}

impl AssetReader for ProfileAssetReader {
    async fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<impl Reader + 'a, AssetReaderError> {
        Ok(VecReader::new(self.read_bytes(path)?))
    }

    async fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<impl Reader + 'a, AssetReaderError> {
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<Box<PathStream>, AssetReaderError> {
        Err(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> std::result::Result<bool, AssetReaderError> {
        Ok(false)
    }
}

#[derive(Clone)]
pub struct ProfileLooseAssetReader {
    profile: ProfileAssetReader,
    loose: LooseFiles,
}

impl AssetReader for ProfileLooseAssetReader {
    async fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<impl Reader + 'a, AssetReaderError> {
        let name = internal_path(path)?.replace('/', "\\");
        let bytes = match self.loose.find(&name, LooseOpen::Index) {
            Some(file) => file
                .read()
                .ok_or_else(|| AssetReaderError::NotFound(path.to_path_buf()))?,
            None => self.profile.read_bytes(path)?,
        };
        Ok(VecReader::new(bytes))
    }

    async fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<impl Reader + 'a, AssetReaderError> {
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<Box<PathStream>, AssetReaderError> {
        Err(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> std::result::Result<bool, AssetReaderError> {
        Ok(false)
    }
}

/// Open the manifest's archives as a `benilla_formats::Chain` for DBC/catalog readers and the
/// synchronous asset cache. This is the same archive set the Bevy source sees, in manifest order.
pub fn open_profile_chain(
    data_dir: &Path,
    manifest_path: &Path,
    profile: &str,
) -> Result<benilla_formats::Chain> {
    let profile = Profile::open(data_dir, manifest_path, profile).with_context(|| {
        format!(
            "opening profile `{profile}` from {}",
            manifest_path.display()
        )
    })?;
    let archives = profile
        .archives()
        .map(|(_, archive)| archive.clone())
        .collect();
    benilla_formats::Chain::from_archives(archives, benilla_formats::Locale::default())
}

/// Register the profile-backed `mpq://` and `mpq-loose://` sources.
///
/// Call instead of [`crate::register_mpq_source`] for the WotLK graphical route; registering both
/// on one app is a duplicate source and is rejected by Bevy.
pub fn register_profile_source(
    app: &mut App,
    data_dir: &Path,
    manifest_path: &Path,
    profile: &str,
) -> Result<()> {
    let reader = ProfileAssetReader::open(data_dir, manifest_path, profile)?;
    let loose = ProfileLooseAssetReader {
        profile: reader.clone(),
        loose: LooseFiles::install(),
    };
    if let Ok(mut readers) = PROFILE_READERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        readers.insert(MPQ_SOURCE.to_owned(), reader.clone());
        readers.insert(MPQ_LOOSE_SOURCE.to_owned(), reader.clone());
    }
    app.register_asset_source(
        MPQ_SOURCE,
        AssetSourceBuilder::new(move || -> Box<dyn ErasedAssetReader> { Box::new(reader.clone()) }),
    );
    app.register_asset_source(
        MPQ_LOOSE_SOURCE,
        AssetSourceBuilder::new(move || -> Box<dyn ErasedAssetReader> { Box::new(loose.clone()) }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(path: &str, size: u64) -> Status {
        Status::Found {
            archive: PathBuf::from(path),
            size,
        }
    }

    /// A required input the candidate does not have is reported by name and class, not as a
    /// generic open failure.
    #[test]
    fn a_missing_required_input_names_the_file_and_its_class() {
        let error = validate_input_from(
            ContentInput::new("DBFilesClient\\Map.dbc"),
            true,
            Status::Missing,
            || panic!("a missing input is never read"),
        )
        .expect_err("a missing input refuses the candidate");
        let text = error.to_string();
        assert!(matches!(error, ContentRevisionError::MissingInput { .. }));
        assert!(text.contains("DBFilesClient\\Map.dbc"), "{text}");
        assert!(text.contains("dbc"), "the class is part of the report: {text}");
    }

    /// A present input with the wrong bytes is `InvalidInput`, and the message names the exact
    /// input and what a coherent file looks like.
    #[test]
    fn an_invalid_header_names_the_input_and_the_expected_signature() {
        let error = validate_input_from(
            ContentInput::new("DBFilesClient\\Map.dbc"),
            true,
            found("common.mpq", 20),
            || Ok(b"NOPE0000000000000000".to_vec()),
        )
        .expect_err("garbage is not a coherent DBC");
        let text = error.to_string();
        assert!(matches!(error, ContentRevisionError::InvalidInput { .. }));
        assert!(text.contains("DBFilesClient\\Map.dbc"), "{text}");
        assert!(text.contains("WDBC"), "{text}");

        let model = validate_input_from(
            ContentInput::new("World\\Goober.m2"),
            true,
            found("patch-w.mpq", 4),
            || Ok(b"XXXX".to_vec()),
        )
        .expect_err("garbage is not a coherent M2");
        assert!(model.to_string().contains("MD20"), "{model}");

        let texture = validate_input_from(
            ContentInput::new("Interface\\Icons\\Foo.blp"),
            true,
            found("patch-w.mpq", 4),
            || Ok(b"JPEG".to_vec()),
        )
        .expect_err("garbage is not a coherent BLP");
        assert!(texture.to_string().contains("BLP2"), "{texture}");
    }

    #[test]
    fn a_ui_input_must_be_non_empty_utf8_text() {
        let empty = validate_input_from(
            ContentInput::new("Interface\\FrameXML\\GlobalStrings.lua"),
            true,
            found("patch-w.mpq", 0),
            || Ok(Vec::new()),
        )
        .expect_err("an empty UI file cannot be loaded");
        assert!(empty.to_string().contains("empty"), "{empty}");

        let binary = validate_input_from(
            ContentInput::new("Interface\\FrameXML\\GlobalStrings.lua"),
            true,
            found("patch-w.mpq", 2),
            || Ok(vec![0xff, 0xff]),
        )
        .expect_err("a UI file that is not text cannot be loaded");
        assert!(binary.to_string().contains("UTF-8"), "{binary}");

        let ok = validate_input_from(
            ContentInput::new("Interface\\FrameXML\\GlobalStrings.lua"),
            true,
            found("patch-w.mpq", 12),
            || Ok(b"SPELL_FAILED".to_vec()),
        )
        .expect("text is a coherent UI input");
        assert_eq!(
            ok.served,
            Some((PathBuf::from("patch-w.mpq"), 12)),
            "the winning archive is the input's provenance"
        );
    }

    /// A tombstone is its own failure: the file is there but deliberately hidden, which is a
    /// different fix from "add the file".
    #[test]
    fn a_tombstoned_input_is_reported_as_tombstoned_not_missing() {
        let error = validate_input_from(
            ContentInput::new("DBFilesClient\\Map.dbc"),
            true,
            Status::Tombstoned {
                archive: PathBuf::from("patch-z.mpq"),
            },
            || panic!("a tombstoned input is never read"),
        )
        .expect_err("a tombstone refuses the candidate");
        let text = error.to_string();
        assert!(matches!(error, ContentRevisionError::TombstonedInput { .. }));
        assert!(text.contains("patch-z.mpq"), "{text}");
    }

    /// The no-regression rule: an input the running revision cannot serve today is not a reload
    /// requirement, or a profile that never had it could never be reloaded.
    #[test]
    fn an_input_the_active_revision_cannot_serve_is_not_a_reload_requirement() {
        let skipped = validate_input_from(
            ContentInput::new("DBFilesClient\\Missing.dbc"),
            false,
            Status::Missing,
            || panic!("nothing to read"),
        )
        .expect("the active revision does not serve it either");
        assert_eq!(skipped.served, None);
        assert_eq!(skipped.input.kind, ContentInputKind::Dbc);
    }

    #[test]
    fn a_manifest_that_names_one_archive_twice_is_ambiguous() {
        let paths = vec![
            PathBuf::from("common.mpq"),
            PathBuf::from("patch-w.mpq"),
            PathBuf::from("common.mpq"),
        ];
        let duplicate = first_duplicate_archive(paths.into_iter())
            .expect("the second entry for one archive is the ambiguous one");
        assert_eq!(duplicate, PathBuf::from("common.mpq"));
        assert_eq!(
            first_duplicate_archive(
                vec![PathBuf::from("common.mpq"), PathBuf::from("patch-w.mpq")].into_iter()
            ),
            None,
            "distinct patch layers keep their manifest precedence"
        );
    }

    /// The commit guard: a candidate staged on an older revision cannot replace the current one.
    #[test]
    fn a_candidate_staged_on_a_replaced_revision_is_superseded() {
        let error = check_commit_base(3, 4).expect_err("a stale base cannot commit");
        assert!(matches!(
            error,
            ContentRevisionError::Superseded {
                staged_base: 3,
                active: 4
            }
        ));
        assert!(check_commit_base(4, 4).is_ok());
    }

    /// Paths class themselves; a caller lists paths, not classifiers.
    #[test]
    fn a_path_names_its_own_content_class() {
        assert_eq!(
            ContentInputKind::of_path("DBFilesClient\\Map.dbc"),
            ContentInputKind::Dbc
        );
        assert_eq!(
            ContentInputKind::of_path("World\\Foo.M2"),
            ContentInputKind::Model
        );
        assert_eq!(
            ContentInputKind::of_path("Interface\\Icons\\Foo.blp"),
            ContentInputKind::Texture
        );
        assert_eq!(
            ContentInputKind::of_path("Interface\\FrameXML\\UIParent.xml"),
            ContentInputKind::Ui
        );
    }

    /// A manifest naming an archive that is not on disk fails at open and names the file, so a
    /// candidate never reaches the commit step half-open. `stage_content_revision` shares this
    /// exact `Profile::open`, so the same refusal is what a reload reports.
    #[test]
    fn a_manifest_that_names_a_missing_archive_fails_with_the_file() {
        let dir = std::env::temp_dir().join(format!("benilla-stage-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let manifest = dir.join("manifest.txt");
        std::fs::write(&manifest, "stock common.mpq\n").expect("write the manifest");
        let error = match ProfileAssetReader::open(&dir, &manifest, "test") {
            Ok(_) => panic!("a manifest naming a missing archive must not open"),
            Err(error) => error,
        };
        let text = format!("{error:#}");
        assert!(text.contains("common.mpq"), "the missing archive is named: {text}");
    }
}
