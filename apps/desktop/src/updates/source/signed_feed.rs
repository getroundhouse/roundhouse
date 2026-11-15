use fetching::Fetcher;
use ring::signature::{ED25519, UnparsedPublicKey};
use std::io;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;
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
    #[error("the update feed's signature doesn't match it, so it may not come from Roundhouse's release workflow")]
    Unsigned,
    #[error("the update feed isn't one this app can read: {0}")]
    Unreadable(#[from] serde_json::Error),
}

/// Velopack only passes on its own error type, so this goes in its `Io` variant, whole, for `problem` to find.
impl From<SourceError> for velopack::Error {
    fn from(error: SourceError) -> Self {
        velopack::Error::Io(io::Error::other(error))
    }
}

/// The update feed's public key, the raw 32 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UpdateFeedKey(pub(crate) [u8; 32]);

impl UpdateFeedKey {
    /// Decodes the key from base64 while compiling, so a build given a malformed one fails, rather than the app.
    pub(crate) const fn from_base64(text: &str) -> Self {
        let text = text.as_bytes();
        assert!(text.len() == 44 && text[43] == b'=' && text[42] != b'=', "a feed key is 32 bytes in base64");
        // Each character is 6 bits. `held` keeps the `bits` that don't make a whole byte yet.
        let (mut key, mut written, mut held, mut bits, mut i) = ([0u8; 32], 0, 0u32, 0u32, 0);
        while i < 43 {
            held = (held << 6) | sextet(text[i]);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                key[written] = low_byte(held >> bits);
                written += 1;
                held &= (1 << bits) - 1;
            }
            i += 1;
        }
        assert!(held == 0, "a feed key's base64 has to end in a 0, 4, 8, A, E, I, M, Q, U, Y, c, g, k, o, s or w");
        UpdateFeedKey(key)
    }
}

const fn sextet(c: u8) -> u32 {
    let value = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("a feed key is base64"),
    };
    value as u32
}

#[expect(clippy::cast_possible_truncation, reason = "the caller has already shifted away all but the low byte")]
const fn low_byte(value: u32) -> u8 {
    value as u8
}

/// The releases the feed lists, once its signature matches `key`. Nothing is read from the feed before that.
pub(crate) fn verified(feed: &[u8], signature: &[u8], key: &UpdateFeedKey) -> Result<VelopackAssetFeed, SourceError> {
    UnparsedPublicKey::new(&ED25519, key.0).verify(feed, signature).map_err(|_| SourceError::Unsigned)?;
    Ok(serde_json::from_slice(feed)?)
}

/// Where Velopack gets updates: the feed only once its signature checks out, and the packages it names from beside
/// it. Neither follows a redirect, so the feed and the packages come from where the app was built to look.
pub(crate) struct SignedSource {
    pub(super) base: String,
    pub(super) key: UpdateFeedKey,
    pub(super) feeds: Fetcher,
    pub(super) packages: Fetcher,
    pub(super) retry_after: Duration,
}

impl SignedSource {
    pub(crate) fn new(base: &str, key: UpdateFeedKey) -> Self {
        Self::waiting(base, key, RETRY_AFTER)
    }

    /// A source that waits `retry_after` before reading again a feed whose signature didn't match.
    pub(crate) fn waiting(base: &str, key: UpdateFeedKey, retry_after: Duration) -> Self {
        let base = base.trim_end_matches('/').to_string();
        Self { base, key, feeds: Fetcher::new(FEED_TIMEOUT), packages: Fetcher::new(PACKAGE_TIMEOUT), retry_after }
    }

    pub(super) fn signed_feed(&self, name: &str) -> Result<VelopackAssetFeed, SourceError> {
        let feed = self.fetch(name, FEED_LIMIT_BYTES)?;
        let signature = self.fetch(&format!("{name}.sig"), SIGNATURE_BYTES)?;
        verified(&feed, &signature, &self.key)
    }

    pub(super) fn fetch(&self, name: &str, limit: u64) -> Result<Vec<u8>, SourceError> {
        Ok(self.feeds.bytes(&format!("{}/{name}", self.base), limit)?)
    }

    pub(super) fn download(&self, asset: &VelopackAsset, path: &Path) -> Result<(), SourceError> {
        Ok(self.packages.to_file(&format!("{}/{}", self.base, asset.FileName), path, asset.Size)?)
    }
}

impl UpdateSource for SignedSource {
    fn get_release_feed(&self, channel: &str, _: &Manifest, _: &str) -> Result<VelopackAssetFeed, velopack::Error> {
        let name = format!("releases.{channel}.json");
        match self.signed_feed(&name) {
            Err(SourceError::Unsigned) => {
                std::thread::sleep(self.retry_after);
                Ok(self.signed_feed(&name)?)
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
