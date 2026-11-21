//! Reading the update feed over HTTPS, and trusting it only once its signature matches the feed's key, as the app does,
//! and downloading the archive of a release's hub from beside it, checked against what the feed says of it.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fetching::{CheckError, Expected, FetchError, Fetched, Fetcher, FolderLock};
use update_feed::{BadChecksum, Feed, HubArchive, Unsigned, UpdateFeedKey};

/// Long enough for a slow connection, short enough that a stalled one doesn't hold up the next read, as the app's.
const FEED_TIMEOUT: Duration = Duration::from_secs(30);
/// Far more than a feed of the releases the workflow keeps, so a broken host can't fill memory, as the app's.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;
const SIGNATURE_BYTES: u64 = 64;

/// How long a download of an archive can go without a byte before it fails. It has no limit as a whole, since that
/// depends on the connection, as the app's download of its package has none.
const ARCHIVE_STALLED: Duration = Duration::from_secs(30);

/// Where a hub installed as a service reads the update feed, and the key its signature has to match.
#[derive(Clone, Debug)]
pub struct UpdateFeed {
    /// Where the feed and the packages beside it are.
    pub base: String,
    pub key: UpdateFeedKey,
    /// Which platform's feed, as `tools/package/publish.py` names it.
    pub channel: &'static str,
}

impl UpdateFeed {
    /// The feed the release workflow publishes, for this build's platform. `None` where no hub is published, as on
    /// Windows.
    pub fn published() -> Option<Self> {
        let channel = update_feed::HUB_CHANNEL?;
        Some(Self { base: update_feed::URL.to_owned(), key: update_feed::KEY, channel })
    }
}

/// Why the feed couldn't be read.
#[derive(Debug, thiserror::Error)]
pub(super) enum ReadError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error(transparent)]
    Unsigned(#[from] Unsigned),
    #[error("the update feed isn't one this hub can read: {0}")]
    Unreadable(#[from] serde_json::Error),
}

/// The feed, once its signature matches. The release workflow uploads the feed and its signature one after the other,
/// so a read can land between them, and one whose signature doesn't match is read again after `retry_after`.
#[expect(clippy::disallowed_methods, reason = "the pause before a feed whose signature didn't match is read again")]
pub(super) fn read(feed: &UpdateFeed, retry_after: Duration) -> Result<Feed, ReadError> {
    let fetcher = Fetcher::new(FEED_TIMEOUT);
    match read_once(&fetcher, feed) {
        Err(ReadError::Unsigned(_)) => {
            std::thread::sleep(retry_after);
            read_once(&fetcher, feed)
        }
        read => read,
    }
}

fn read_once(fetcher: &Fetcher, feed: &UpdateFeed) -> Result<Feed, ReadError> {
    let name = format!("{}/releases.{}.json", feed.base.trim_end_matches('/'), feed.channel);
    let bytes = fetcher.bytes(&name, FEED_LIMIT_BYTES)?;
    let signature = fetcher.bytes(&format!("{name}.sig"), SIGNATURE_BYTES)?;
    feed.key.verify(&bytes, &signature)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Why an archive couldn't be downloaded.
#[derive(Debug, thiserror::Error)]
pub(super) enum FetchArchiveError {
    #[error(transparent)]
    Checksum(#[from] BadChecksum),
    #[error("the update feed names the archive {0}, which isn't a file's name")]
    BadName(String),
    #[error("couldn't take the folder downloads go in: {0}")]
    Folder(std::io::Error),
    #[error(transparent)]
    Fetch(#[from] CheckError),
    #[error("the download stopped before it finished")]
    Stopped,
}

/// Downloads `archive`, from beside `feed`, into `downloads`, and gives where it is, once it's the size and has the
/// SHA-256 the feed's signature covers.
pub(super) fn fetch_archive(
    feed: &UpdateFeed,
    archive: &HubArchive,
    downloads: &Path,
) -> Result<PathBuf, FetchArchiveError> {
    let name = Path::new(&archive.file_name);
    if name.file_name() != Some(name.as_os_str()) {
        return Err(FetchArchiveError::BadName(archive.file_name.clone()));
    }
    let expected = Expected { size_bytes: archive.size, sha256: archive.sha256_bytes()? };
    let lock = FolderLock::take(downloads).map_err(FetchArchiveError::Folder)?;
    let url = format!("{}/{}", feed.base.trim_end_matches('/'), archive.file_name);
    let fetcher = Fetcher::for_big_files(ARCHIVE_STALLED);
    match fetcher.checked_file(&lock, &url, name, &expected, &mut |_| ControlFlow::Continue(()))? {
        Fetched::Done => Ok(lock.dir().join(name)),
        // Nothing here says to stop.
        Fetched::Stopped => Err(FetchArchiveError::Stopped),
    }
}
