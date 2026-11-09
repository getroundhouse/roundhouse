//! The update feed, trusted only once its signature matches the key built into the app. Velopack checks each
//! package against the checksum the feed gives, so whoever could change the feed could otherwise change the app.
//! The release workflow signs the feed's exact bytes with Ed25519, and puts the raw signature beside it, in
//! `releases.<channel>.json.sig`.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

use ring::signature::{ED25519, UnparsedPublicKey};
use velopack::bundle::Manifest;
use velopack::sources::{HttpSource, UpdateSource};
use velopack::{HttpOptions, VelopackAsset, VelopackAssetFeed};

/// Long enough for a slow connection, short enough that a stalled one doesn't hold up the next check.
const FEED_TIMEOUT: Duration = Duration::from_secs(30);
/// A full package is about 19 MB.
const PACKAGE_TIMEOUT: Duration = Duration::from_mins(15);
/// Far more than a feed of the releases the workflow keeps, so a broken host can't fill memory.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;
const SIGNATURE_BYTES: u64 = 64;

#[derive(Debug, thiserror::Error)]
pub(crate) enum FeedError {
    #[error("couldn't fetch {url}: {source}")]
    Fetch { url: String, source: ureq::Error },
    #[error("the update feed's signature doesn't match it")]
    Unsigned,
    #[error("the update feed isn't one this app can read: {0}")]
    Unreadable(#[from] serde_json::Error),
}

impl From<FeedError> for velopack::Error {
    fn from(error: FeedError) -> Self {
        velopack::Error::Other(error.to_string())
    }
}

/// An Ed25519 public key, the raw 32 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FeedKey(pub(crate) [u8; 32]);

impl FeedKey {
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
        FeedKey(key)
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
pub(crate) fn verified(feed: &[u8], signature: &[u8], key: &FeedKey) -> Result<VelopackAssetFeed, FeedError> {
    UnparsedPublicKey::new(&ED25519, key.0).verify(feed, signature).map_err(|_| FeedError::Unsigned)?;
    Ok(serde_json::from_slice(feed)?)
}

/// Where Velopack gets updates: the feed only once its signature checks out, and the packages it names from beside
/// it, which Velopack checks against the feed's checksums.
pub(crate) struct SignedFeed {
    base: String,
    key: FeedKey,
    agent: ureq::Agent,
    packages: HttpSource,
}

impl SignedFeed {
    pub(crate) fn new(base: &str, key: FeedKey) -> Self {
        let base = base.trim_end_matches('/').to_string();
        let agent = ureq::Agent::config_builder().timeout_global(Some(FEED_TIMEOUT)).build().into();
        let timeout = u64::try_from(PACKAGE_TIMEOUT.as_millis()).unwrap_or(u64::MAX);
        let packages =
            HttpSource::new_with_options(&base, HttpOptions { TimeoutMilliseconds: timeout, ..Default::default() });
        Self { base, key, agent, packages }
    }

    fn fetch(&self, name: &str, limit: u64) -> Result<Vec<u8>, FeedError> {
        let url = format!("{}/{name}", self.base);
        // Anything longer is an error, rather than cut to fit. ureq also refuses a body exactly as long as its limit.
        let fetched = self
            .agent
            .get(&url)
            .call()
            .and_then(|mut response| response.body_mut().with_config().limit(limit + 1).read_to_vec());
        fetched.map_err(|source| FeedError::Fetch { url, source })
    }
}

impl UpdateSource for SignedFeed {
    fn get_release_feed(&self, channel: &str, _: &Manifest, _: &str) -> Result<VelopackAssetFeed, velopack::Error> {
        let name = format!("releases.{channel}.json");
        let feed = self.fetch(&name, FEED_LIMIT_BYTES)?;
        let signature = self.fetch(&format!("{name}.sig"), SIGNATURE_BYTES)?;
        Ok(verified(&feed, &signature, &self.key)?)
    }

    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        local_file: &Path,
        progress: Option<Sender<i16>>,
    ) -> Result<(), velopack::Error> {
        self.packages.download_release_entry(asset, local_file, progress)
    }
}

#[cfg(test)]
mod tests;
