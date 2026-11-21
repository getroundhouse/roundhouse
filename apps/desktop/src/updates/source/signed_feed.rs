use super::{ToKeep, only_taking_over};
use fetching::Fetcher;
use std::io;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;
use update_feed::{Feed, Unsigned, UpdateFeedKey};
use velopack::bundle::Manifest;
use velopack::sources::UpdateSource;
use velopack::{VelopackAsset, VelopackAssetFeed};

/// Long enough for a slow connection, short enough that a stalled one doesn't hold up the next check.
const FEED_TIMEOUT: Duration = Duration::from_secs(30);
/// A full package is about 20 MB.
const PACKAGE_TIMEOUT: Duration = Duration::from_mins(15);
/// Far more than a feed of the releases the workflow keeps, so a broken host can't fill memory.
pub(super) const FEED_LIMIT_BYTES: u64 = 1024 * 1024;
const SIGNATURE_BYTES: u64 = 64;
/// How long to wait before reading a feed again whose signature didn't match. The release workflow uploads the feed
/// and its signature one after the other, so a check can land between them.
const RETRY_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceError {
    #[error(transparent)]
    Fetch(#[from] fetching::FetchError),
    #[error(transparent)]
    Unsigned(#[from] Unsigned),
    #[error("the update feed isn't one this app can read: {0}")]
    Unreadable(#[from] serde_json::Error),
}

/// Velopack only passes on its own error type, so this goes in its `Io` variant, whole, for `problem` to find.
impl From<SourceError> for velopack::Error {
    fn from(error: SourceError) -> Self {
        velopack::Error::Io(io::Error::other(error))
    }
}

/// The releases the feed lists, once its signature matches `key`. Nothing is read from the feed before that.
pub(crate) fn verified(feed: &[u8], signature: &[u8], key: &UpdateFeedKey) -> Result<VelopackAssetFeed, SourceError> {
    key.verify(feed, signature)?;
    Ok(serde_json::from_slice(feed)?)
}

/// Where Velopack gets updates: the feed only once its signature checks out, and the packages it names from beside
/// it. Neither follows a redirect, so the feed and the packages come from where the app was built to look. The feed
/// it gives Velopack leaves out each release an update from this app's can't take: one whose hub can't talk to the
/// workers `keeping` says to keep, or that doesn't update from this app's release and its hub's.
pub(crate) struct SignedSource {
    pub(super) base: String,
    pub(super) key: UpdateFeedKey,
    pub(super) keeping: ToKeep,
    pub(super) feeds: Fetcher,
    pub(super) packages: Fetcher,
    pub(super) retry_after: Duration,
}

impl SignedSource {
    pub(crate) fn new(base: &str, key: UpdateFeedKey, keeping: ToKeep) -> Self {
        Self::waiting(base, key, keeping, RETRY_AFTER)
    }

    /// A source that waits `retry_after` before reading again a feed whose signature didn't match.
    pub(crate) fn waiting(base: &str, key: UpdateFeedKey, keeping: ToKeep, retry_after: Duration) -> Self {
        let base = base.trim_end_matches('/').to_string();
        let (feeds, packages) = (Fetcher::new(FEED_TIMEOUT), Fetcher::new(PACKAGE_TIMEOUT));
        Self { base, key, keeping, feeds, packages, retry_after }
    }

    /// The releases the feed `name` lists that an update from the app's release, `version`, can take, keeping what
    /// `keeping` says.
    pub(super) fn signed_feed(&self, name: &str, version: &str) -> Result<VelopackAssetFeed, SourceError> {
        let feed = self.fetch(name, FEED_LIMIT_BYTES)?;
        let signature = self.fetch(&format!("{name}.sig"), SIGNATURE_BYTES)?;
        let releases = verified(&feed, &signature, &self.key)?;
        let keeping = self.keeping.now();
        Ok(only_taking_over(releases, &serde_json::from_slice::<Feed>(&feed)?, keeping.running(version)))
    }

    pub(super) fn fetch(&self, name: &str, limit: u64) -> Result<Vec<u8>, SourceError> {
        Ok(self.feeds.bytes(&format!("{}/{name}", self.base), limit)?)
    }

    pub(super) fn download(&self, asset: &VelopackAsset, path: &Path) -> Result<(), SourceError> {
        Ok(self.packages.to_file(&format!("{}/{}", self.base, asset.FileName), path, asset.Size)?)
    }
}

impl UpdateSource for SignedSource {
    #[expect(
        clippy::disallowed_methods,
        reason = "the pause before a feed whose signature didn't match is read again, as one being published can be"
    )]
    fn get_release_feed(&self, channel: &str, app: &Manifest, _: &str) -> Result<VelopackAssetFeed, velopack::Error> {
        let (name, version) = (format!("releases.{channel}.json"), app.version.to_string());
        match self.signed_feed(&name, &version) {
            Err(SourceError::Unsigned(_)) => {
                std::thread::sleep(self.retry_after);
                Ok(self.signed_feed(&name, &version)?)
            }
            read => Ok(read?),
        }
    }

    /// Velopack checks the package's checksum once this returns. Nothing shows a download's progress, so this doesn't
    /// report it.
    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        local_file: &Path,
        _: Option<Sender<i16>>,
    ) -> Result<(), velopack::Error> {
        let downloaded = self.download(asset, local_file);
        if downloaded.is_err() {
            // Velopack would otherwise leave it until the next download that works.
            let _ = std::fs::remove_file(local_file);
        }
        Ok(downloaded?)
    }
}
