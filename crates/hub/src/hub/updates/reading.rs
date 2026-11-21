//! Reading the update feed over HTTPS, and trusting it only once its signature matches the feed's key, as the app does.

use std::time::Duration;

use fetching::{FetchError, Fetcher};
use update_feed::{Feed, Unsigned, UpdateFeedKey};

/// Long enough for a slow connection, short enough that a stalled one doesn't hold up the next read, as the app's.
const FEED_TIMEOUT: Duration = Duration::from_secs(30);
/// Far more than a feed of the releases the workflow keeps, so a broken host can't fill memory, as the app's.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;
const SIGNATURE_BYTES: u64 = 64;

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
