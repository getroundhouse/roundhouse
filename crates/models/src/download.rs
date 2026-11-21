use std::io;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fetching::{CheckError, Expected, FetchError, Fetched, Fetcher, FolderLock};

use super::ModelFiles;

/// How long connecting, or a read waiting for the next of a file, can take before the download gives up, so a stalled
/// connection says so.
const STALLED: Duration = Duration::from_secs(60);

/// How much of a model is here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done_bytes: u64,
    pub total_bytes: u64,
}

/// Why a model's download failed.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error(transparent)]
    Folder(#[from] FolderError),
    #[error(transparent)]
    Check(#[from] CheckError),
}

impl DownloadError {
    /// Why the model didn't come, in a few words for a person, finishing a sentence such as "Couldn't download the
    /// model:", with `here` naming the device, such as "this Mac".
    pub fn why(&self, here: &str) -> String {
        let fetch = match self {
            DownloadError::Folder(folder) => return not_saved(&folder.source, here),
            DownloadError::Check(CheckError::NotTheFile { .. }) => {
                return "what came wasn't the file it should be.".into();
            }
            DownloadError::Check(CheckError::Fetch(fetch)) => fetch,
        };
        if fetch.timed_out() {
            return "the connection stalled.".to_string();
        }
        match fetch {
            FetchError::Request { .. } => "Hugging Face couldn't be reached.".to_string(),
            FetchError::Status { status, .. } => format!("Hugging Face answered {status}."),
            FetchError::Read { .. } | FetchError::CutShort { .. } => "the connection was lost.".to_string(),
            FetchError::TooLong { .. } => "what came wasn't the file it should be.".to_string(),
            FetchError::Save { source, .. } => not_saved(source, here),
        }
    }
}

/// Why a file couldn't be saved on the device `here`.
pub(crate) fn not_saved(source: &io::Error, here: &str) -> String {
    if source.kind() == io::ErrorKind::StorageFull {
        return format!("there isn't room for it on {here}.");
    }
    format!("it couldn't be saved: {source}.")
}

/// A folder of the model's that couldn't be used, which is how deleting it fails.
#[derive(Debug, thiserror::Error)]
#[error("couldn't use {path}: {source}")]
pub struct FolderError {
    pub path: PathBuf,
    pub source: io::Error,
}

impl ModelFiles {
    /// Downloads into its folder under `model_dir` each of its files that isn't there yet, from `from`, which serves
    /// each at its path, as [`Self::hugging_face`] does. It holds `model_dir`'s lock meanwhile, so another download
    /// into it, or [`delete`], in this process or another, waits for it to let go. `model_dir` is kept out of backups,
    /// since what's in it can be downloaded again. Once every file is here, other revisions' folders are removed.
    ///
    /// `progress` is told how much of the model is here, once the lock is taken and as it grows, and stops the
    /// download by giving `Break`, which keeps what's come of a file for the next download to go on from.
    pub fn download(
        &self,
        model_dir: &Path,
        from: &str,
        progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<Fetched, DownloadError> {
        let lock = FolderLock::take(model_dir).map_err(folder(model_dir))?;
        exclude_from_backups(model_dir);
        let total_bytes = self.size_bytes();
        let missing: Vec<_> = self.files.iter().filter(|file| !self.has(model_dir, file)).collect();
        let mut done_bytes = total_bytes - missing.iter().map(|file| file.size_bytes).sum::<u64>();
        if progress(Progress { done_bytes, total_bytes }).is_break() {
            return Ok(Fetched::Stopped);
        }
        let fetcher = Fetcher::for_big_files(STALLED);
        for file in missing {
            let (url, to) = (format!("{from}/{}", file.path), Path::new(self.revision).join(file.path));
            let expected = Expected { size_bytes: file.size_bytes, sha256: file.sha256 };
            let mut told = |here| progress(Progress { done_bytes: done_bytes + here, total_bytes });
            if fetcher.checked_file(&lock, &url, &to, &expected, &mut told)? == Fetched::Stopped {
                return Ok(Fetched::Stopped);
            }
            done_bytes += file.size_bytes;
        }
        // What's left stays until the next download, which tries again.
        let _ = remove_revisions(&lock, |revision| revision != self.revision);
        Ok(Fetched::Done)
    }
}

/// Removes every revision of the model from `model_dir`, once it holds its lock, so a download that's stopping lets go
/// first. `model_dir` and its lock stay.
pub fn delete(model_dir: &Path) -> Result<(), FolderError> {
    let lock = FolderLock::take(model_dir).map_err(folder(model_dir))?;
    remove_revisions(&lock, |_| true)
}

/// Removes the folders in `lock`'s folder whose revision `remove` says to, and leaves everything else, its lock too.
fn remove_revisions(lock: &FolderLock, remove: impl Fn(&str) -> bool) -> Result<(), FolderError> {
    for entry in std::fs::read_dir(lock.dir()).map_err(folder(lock.dir()))? {
        let path = entry.map_err(folder(lock.dir()))?.path();
        if path.is_dir() && path.file_name().and_then(|name| name.to_str()).is_some_and(&remove) {
            std::fs::remove_dir_all(&path).map_err(folder(&path))?;
        }
    }
    Ok(())
}

fn folder(path: &Path) -> impl FnOnce(io::Error) -> FolderError + '_ {
    move |source| FolderError { path: path.to_path_buf(), source }
}

/// Marks `dir` as not to be backed up, by Time Machine and iCloud, which read the same key on iOS. A failure only means
/// it's backed up.
#[cfg(target_os = "macos")]
fn exclude_from_backups(dir: &Path) {
    use objc2_foundation::{NSNumber, NSString, NSURL, NSURLIsExcludedFromBackupKey};
    let Some(path) = dir.to_str() else { return };
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    // SAFETY: the key's value is a boolean `NSNumber`, as Foundation documents it.
    let _ = unsafe { url.setResourceValue_forKey_error(Some(&NSNumber::new_bool(true)), NSURLIsExcludedFromBackupKey) };
}

/// On Windows and Linux, no backup reads a mark like it.
#[cfg(not(target_os = "macos"))]
fn exclude_from_backups(_: &Path) {}
