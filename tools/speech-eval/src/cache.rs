//! The eval's cache, outside the repository: the datasets' indexes, `LibriSpeech`'s recordings, each Tech-Sentences
//! clip once it's been fetched, and the model, unless the run is given its files. Every file is from a pinned
//! revision, so a run hears what the baseline's run heard.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, bail};
use fetching::{Expected, Fetched, Fetcher, FolderLock};
use models::ModelFile;

/// How long a read can take before a download is given up on.
const STALLED: Duration = Duration::from_secs(60);

/// A file the eval downloads whole from `url`, checked against its size and SHA-256 as a model's files are, and kept
/// at its path in the cache.
pub(crate) struct Pinned {
    pub(crate) url: &'static str,
    pub(crate) file: ModelFile,
}

/// The cache, held by this run alone.
pub(crate) struct Cache {
    lock: FolderLock,
    fetcher: Fetcher,
}

impl Cache {
    /// The cache in `dir`, once no other run holds it.
    pub(crate) fn open(dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(dir).with_context(|| format!("couldn't make {}", dir.display()))?;
        let lock = FolderLock::take(dir).with_context(|| format!("couldn't lock {}", dir.display()))?;
        Ok(Self { lock, fetcher: Fetcher::for_big_files(STALLED) })
    }

    pub(crate) fn dir(&self) -> &Path {
        self.lock.dir()
    }

    /// Where `pinned` is, downloaded first if it isn't here.
    pub(crate) fn file(&self, pinned: &Pinned) -> anyhow::Result<PathBuf> {
        let Pinned { url, file } = pinned;
        let at = self.dir().join(file.path);
        if at.exists() {
            return Ok(at);
        }
        eprintln!("downloading {url} ({} MB)", file.size_bytes / 1_000_000);
        let expected = Expected { size_bytes: file.size_bytes, sha256: file.sha256 };
        let fetched = self
            .fetcher
            .checked_file(&self.lock, url, Path::new(file.path), &expected, &mut |_| ControlFlow::Continue(()));
        match fetched.with_context(|| format!("couldn't download {url}"))? {
            Fetched::Done => Ok(at),
            Fetched::Stopped => bail!("the download of {url} stopped"),
        }
    }

    /// `length` bytes of the file at `url` from `start`, kept at `path` in the cache once they've come.
    pub(crate) fn range(&self, url: &str, start: u64, length: u64, path: &str) -> anyhow::Result<Vec<u8>> {
        let at = self.dir().join(path);
        if let Ok(bytes) = std::fs::read(&at) {
            return Ok(bytes);
        }
        let bytes = self.fetcher.range(url, start, length).with_context(|| format!("couldn't fetch from {url}"))?;
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("couldn't make {}", parent.display()))?;
        }
        // Written beside it and moved into place, so a run stopped as it writes leaves nothing half there.
        let part = at.with_extension("part");
        std::fs::write(&part, &bytes)
            .and_then(|()| std::fs::rename(&part, &at))
            .with_context(|| format!("couldn't keep {}", at.display()))?;
        Ok(bytes)
    }
}

/// The cache's folder unless a run is given another: the user's caches, where the system can clear it.
pub(crate) fn default_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME isn't set, so pass --cache")?;
    let caches = if cfg!(target_os = "macos") { "Library/Caches" } else { ".cache" };
    Ok(Path::new(&home).join(caches).join("roundhouse-speech-eval"))
}
