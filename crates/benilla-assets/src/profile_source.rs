//! Bevy `AssetSource` readers over a manifest-defined `benilla_profile::Profile`.
//!
//! This is the 3.3.5a counterpart of [`crate::MpqAssetReader`]: archive precedence, disabled
//! toggles and provenance come from the owner's manifest instead of the vanilla auto-discovered
//! chain. It does not write the install and does not discover archives.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use anyhow::{Context, Result};
use benilla_profile::{Profile, Status};
use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetSourceBuilder, ErasedAssetReader, PathStream, Reader,
    VecReader,
};
use bevy::asset::AssetApp;
use bevy::prelude::*;

use crate::{
    internal_path, LooseFiles, LooseOpen, MPQ_LOOSE_SOURCE, MPQ_SOURCE,
};

#[derive(Clone)]
pub struct ProfileAssetReader {
    profile: Arc<RwLock<Profile>>,
    manifest_path: PathBuf,
}

/// Registered readers by Bevy source name. An M2 loader needs a sibling `.skin` while it already
/// holds the `.m2` bytes; Bevy's `LoadContext` exposes no second read, so the profile registered on
/// the app is also reachable synchronously here. One process, one route profile.
static PROFILE_READERS: OnceLock<Mutex<HashMap<String, ProfileAssetReader>>> = OnceLock::new();

/// Read `path` synchronously from the profile registered as `source`.
pub(crate) fn read_profile_sync(source: &str, path: &Path) -> Option<Vec<u8>> {
    let readers = PROFILE_READERS.get()?.lock().ok()?;
    let reader = readers.get(source)?.clone();
    drop(readers);
    reader.read_bytes(path).ok()
}

impl ProfileAssetReader {
    pub fn open(data_dir: &Path, manifest_path: &Path, profile: &str) -> Result<Self> {
        let opened = Profile::open(data_dir, manifest_path, profile)
            .with_context(|| format!("opening profile `{profile}` from {}", manifest_path.display()))?;
        Ok(Self {
            profile: Arc::new(RwLock::new(opened)),
            manifest_path: manifest_path.to_path_buf(),
        })
    }

    /// Candidate-first reload seam: a failed reopen leaves the existing profile active.
    pub fn reload(&self) -> Result<()> {
        self.profile
            .write()
            .map_err(|_| anyhow::anyhow!("profile lock poisoned"))?
            .reload()
            .with_context(|| format!("reloading {}", self.manifest_path.display()))
    }

    pub fn status(&self, name: &str) -> Result<Status> {
        Ok(self
            .profile
            .read()
            .map_err(|_| anyhow::anyhow!("profile lock poisoned"))?
            .status(name))
    }

    fn read_bytes(&self, path: &Path) -> std::result::Result<Vec<u8>, AssetReaderError> {
        let internal = internal_path(path)?;
        self.profile
            .read()
            .map_err(|_| {
                AssetReaderError::Io(Arc::new(std::io::Error::other(
                    "profile lock poisoned",
                )))
            })?
            .read(&internal)
            .map_err(|error| {
                AssetReaderError::Io(Arc::new(std::io::Error::other(format!("{error:#}"))))
            })
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
