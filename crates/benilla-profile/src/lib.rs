//! Manifest-defined MPQ patch chains for the 3.3.5a install profiles.
//!
//! The vanilla [`benilla_formats::Chain`] carves the 1.12.1 mount order out of the reference
//! client; this crate is its 3.3.5a counterpart where the order comes from an explicit manifest
//! file instead of discovery, because this install carries owner-toggled custom archives
//! (`.disabled` renames, lettered patches) that auto-discovery would either load wrongly or
//! silently skip. The loader never writes to, renames in, or deletes from the `Data` directory.
//!
//! Deviation from the vanilla chain, on purpose: a manifest entry whose archive is absent or
//! `.disabled` on disk is a hard error naming the file, so a toggled-off patch can never be
//! loaded behind the owner's back, and a typo'd manifest fails at open, not as missing files
//! far downstream.
//!
//! The manifest is line-based: one archive per line, `stock <name.mpq>` or `wm <name.mpq>`,
//! ascending precedence (later lines win), `#` comments. The `stock`/`wm` origin tag feeds
//! failure classification and provenance reporting; it asserts nothing about file contents.
//!
//! Reload seam (Project B goal C1, client-side hot-patching): [`Profile::reload`] re-reads the
//! manifest and reopens archives, so a content publication can swap profiles or archive sets in
//! a running process without a restart. Callers own the swap point.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use benilla_mpq::Archive;

mod classify;
pub use classify::{Class, Side, classify, classify_sides};

/// One manifest line: an archive's disk name and its declared origin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Stock,
    Wm,
}

/// A parsed manifest: the profile's name and its archives in ascending precedence.
pub struct Manifest {
    pub profile: String,
    pub entries: Vec<(Origin, PathBuf)>,
}

/// The result of resolving one internal path in one profile.
#[derive(Clone, Debug)]
pub enum Status {
    /// Readable from the named archive (provenance) at this size.
    Found { archive: PathBuf, size: u64 },
    /// No archive in the profile has the path.
    Missing,
    /// The winning archive tombstones the path (delete marker): unreadable by design.
    Tombstoned { archive: PathBuf },
}

impl Manifest {
    /// Parse a manifest file. Every entry must name an archive relative to the `Data` directory.
    pub fn parse(text: &str, profile: &str) -> Result<Self> {
        let mut entries = Vec::new();
        for (number, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (origin, name) = line
                .split_once(char::is_whitespace)
                .with_context(|| format!("line {}: expected `stock <name.mpq>` or `wm <name.mpq>`", number + 1))?;
            let origin = match origin {
                "stock" => Origin::Stock,
                "wm" => Origin::Wm,
                other => bail!("line {}: unknown origin `{other}`", number + 1),
            };
            let name = name.trim();
            if name.is_empty() || !name.to_ascii_lowercase().ends_with(".mpq") {
                bail!("line {}: `{name}` is not an .mpq archive name", number + 1);
            }
            entries.push((origin, PathBuf::from(name)));
        }
        if entries.is_empty() {
            bail!("manifest for `{profile}` lists no archives");
        }
        Ok(Self { profile: profile.to_string(), entries })
    }
}

/// An opened profile: the manifest's archives, ready to resolve and read.
pub struct Profile {
    data_dir: PathBuf,
    manifest_path: PathBuf,
    manifest: Manifest,
    archives: Vec<(Origin, Archive)>,
}

impl Profile {
    /// Parse the manifest at `manifest_path` and open every archive under `data_dir`.
    pub fn open(data_dir: &Path, manifest_path: &Path, profile: &str) -> Result<Self> {
        let text = std::fs::read_to_string(manifest_path)
            .with_context(|| format!("reading manifest {}", manifest_path.display()))?;
        let manifest = Manifest::parse(&text, profile)?;
        let mut this = Self {
            data_dir: data_dir.to_path_buf(),
            manifest_path: manifest_path.to_path_buf(),
            manifest,
            archives: Vec::new(),
        };
        this.open_archives()?;
        Ok(this)
    }

    fn open_archives(&mut self) -> Result<()> {
        let mut archives = Vec::new();
        for (origin, name) in &self.manifest.entries {
            let path = self.data_dir.join(name);
            // An owner-toggled archive stays off: refuse rather than reach for a `.disabled`
            // twin or open it anyway. The message names both facts so the fix is obvious.
            if !path.is_file() {
                if path.with_extension("mpq.disabled").is_file() {
                    bail!(
                        "archive {} of profile `{}` is disabled by the owner ({}); \
                         edit the manifest to honor the toggle",
                        name.display(),
                        self.manifest.profile,
                        path.with_extension("mpq.disabled").display()
                    );
                }
                bail!("archive {} of profile `{}` not found in {}", name.display(), self.manifest.profile, self.data_dir.display());
            }
            archives.push((*origin, Archive::open(&path).with_context(|| format!("opening {}", path.display()))?));
        }
        self.archives = archives;
        Ok(())
    }

    /// Re-read the manifest and reopen every archive (the C1 hot-swap seam). Files already read
    /// stay valid; only later resolutions see the new set.
    pub fn reload(&mut self) -> Result<()> {
        let text = std::fs::read_to_string(&self.manifest_path)
            .with_context(|| format!("re-reading manifest {}", self.manifest_path.display()))?;
        self.manifest = Manifest::parse(&text, &self.manifest.profile)?;
        self.open_archives()
    }

    /// The profile's declared locale tag (informational; string tables are the reader's concern).
    pub fn profile(&self) -> &str {
        &self.manifest.profile
    }

    fn resolve(&self, name: &str) -> Option<&(Origin, Archive)> {
        self.archives.iter().rev().find(|(_, a)| a.contains(name))
    }

    /// Resolve one internal path (`/` or `\`, any case) to its status and provenance.
    pub fn status(&self, name: &str) -> Status {
        match self.resolve(name) {
            Some((_, archive)) if archive.is_delete_marker(name) => {
                Status::Tombstoned { archive: archive.path().to_path_buf() }
            }
            Some((_, archive)) => Status::Found {
                archive: archive.path().to_path_buf(),
                size: archive.file_size(name).unwrap_or(0) as u64,
            },
            None => Status::Missing,
        }
    }

    /// Read one internal path from its winning archive, with provenance in every error.
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let (_, archive) = self
            .resolve(name)
            .ok_or_else(|| anyhow::anyhow!("file not in profile `{}`: {name}", self.manifest.profile))?;
        if archive.is_delete_marker(name) {
            bail!("file tombstoned in `{}` by {}: {name}", self.manifest.profile, archive.path().display());
        }
        archive
            .read_file(name)
            .with_context(|| format!("reading {name} from {}", archive.path().display()))
    }
}

impl Profile {
    /// The declared origin of the archive a path resolves to (provenance for classification).
    pub fn origin_of(&self, name: &str) -> Option<Origin> {
        self.resolve(name).map(|(origin, _)| *origin)
    }
}

impl Profile {
    /// The opened archives with their declared origins, ascending precedence (dev/CLI access).
    pub fn archives(&self) -> impl Iterator<Item = (Origin, &Archive)> {
        self.archives.iter().map(|(origin, archive)| (*origin, archive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_parses_origins_comments_and_order() {
        let m = Manifest::parse(
            "# comment\nstock common.mpq\n\nwm patch-z.mpq\n",
            "test",
        )
        .unwrap();
        assert_eq!(m.entries.len(), 2);
        assert_eq!(m.entries[0], (Origin::Stock, PathBuf::from("common.mpq")));
        assert_eq!(m.entries[1], (Origin::Wm, PathBuf::from("patch-z.mpq")));
    }

    #[test]
    fn a_bad_origin_or_name_is_rejected_with_its_line() {
        assert!(Manifest::parse("custom patch-x.mpq", "t").is_err());
        assert!(Manifest::parse("stock patch-x.txt", "t").is_err());
        assert!(Manifest::parse("stock", "t").is_err());
        assert!(Manifest::parse("", "t").is_err());
    }

    #[test]
    fn classification_core_covers_every_side_pair() {
        use classify::{classify_sides, Class, Side};
        use Status::{Found, Missing, Tombstoned};
        let found = |origin| Side {
            status: Found { archive: PathBuf::from("a.mpq"), size: 1 },
            origin: Some(origin),
        };
        let missing = || Side { status: Missing, origin: None };
        assert_eq!(classify_sides(&missing(), &missing()), Class::Missing);
        assert_eq!(classify_sides(&missing(), &found(Origin::Wm)), Class::WmOnly);
        assert_eq!(classify_sides(&missing(), &found(Origin::Stock)), Class::StockOnly);
        assert_eq!(classify_sides(&found(Origin::Stock), &found(Origin::Wm)), Class::Shared);
        assert_eq!(
            classify_sides(
                &Side { status: Tombstoned { archive: PathBuf::from("p.mpq") }, origin: None },
                &missing(),
            ),
            Class::Missing,
        );
    }
}
