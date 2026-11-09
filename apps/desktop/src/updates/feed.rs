//! Where the app gets updates: the feed, trusted only once its signature matches the key built into the app, and the
//! packages it names. The release workflow signs the feed's exact bytes with Ed25519, and puts the raw signature beside
//! it, in `releases.<channel>.json.sig`.
//!
//! Velopack checks a package's size and checksum against the feed once it has downloaded it, so a changed package is
//! refused, and nothing longer than the feed says is downloaded at all. Velopack installs whatever is at the package's
//! path when the app quits, without checking it again, so something running as this user could swap it, but it could
//! as well change the app itself, which this user owns.

use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use ring::signature::{ED25519, UnparsedPublicKey};
use velopack::bundle::Manifest;
use velopack::sources::UpdateSource;
use velopack::{VelopackAsset, VelopackAssetFeed};

use super::{Kind, Step};

/// Long enough for a slow connection, short enough that a stalled one doesn't hold up the next check.
const FEED_TIMEOUT: Duration = Duration::from_secs(30);
/// A full package is about 20 MB.
const PACKAGE_TIMEOUT: Duration = Duration::from_mins(15);
/// Far more than a feed of the releases the workflow keeps, so a broken host can't fill memory.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;
const SIGNATURE_BYTES: u64 = 64;
/// How long to wait before reading a feed again whose signature didn't match. The release workflow uploads the feed
/// and its signature one after the other, so a check can land between them.
const RETRY_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub(crate) enum FeedError {
    #[error("couldn't fetch {url}: {source}")]
    Fetch { url: String, source: ureq::Error },
    #[error("{url} answered {status} rather than the file")]
    Status { url: String, status: u16 },
    #[error("couldn't read {url}: {source}")]
    Read { url: String, source: io::Error },
    #[error("{url} ended after {got} of its {expected} bytes")]
    CutShort { url: String, expected: u64, got: u64 },
    #[error("{url} is longer than the {limit} bytes it should be")]
    TooLong { url: String, limit: u64 },
    #[error("the update feed's signature doesn't match it, so it may not come from Roundhouse's release workflow")]
    Unsigned,
    #[error("the update feed isn't one this app can read: {0}")]
    Unreadable(#[from] serde_json::Error),
    #[error("couldn't save {path}: {source}")]
    Save { path: PathBuf, source: io::Error },
}

impl FeedError {
    /// What the sidebar makes of it.
    pub(crate) fn kind(&self, step: Step) -> Kind {
        match self {
            Self::Fetch { .. } | Self::Status { .. } | Self::Read { .. } | Self::CutShort { .. } => Kind::Unreachable,
            Self::TooLong { .. } => Kind::Refused("a download is longer than it should be"),
            Self::Unsigned => Kind::Refused("the feed's signature doesn't match"),
            Self::Unreadable(_) | Self::Save { .. } => Kind::Failed(step),
        }
    }
}

/// Velopack only passes on its own error type, so this goes in its `Io` variant, whole, for `super::problem` to find.
impl From<FeedError> for velopack::Error {
    fn from(error: FeedError) -> Self {
        velopack::Error::Io(io::Error::other(error))
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
/// it. Neither follows a redirect, so the feed and the packages come from where the app was built to look.
pub(crate) struct SignedFeed {
    base: String,
    key: FeedKey,
    feeds: ureq::Agent,
    packages: ureq::Agent,
    retry_after: Duration,
}

impl SignedFeed {
    pub(crate) fn new(base: &str, key: FeedKey) -> Self {
        Self::waiting(base, key, RETRY_AFTER)
    }

    /// A source that waits `retry_after` before reading again a feed whose signature didn't match.
    pub(crate) fn waiting(base: &str, key: FeedKey, retry_after: Duration) -> Self {
        let agent =
            |timeout| ureq::Agent::config_builder().timeout_global(Some(timeout)).max_redirects(0).build().into();
        let base = base.trim_end_matches('/').to_string();
        Self { base, key, feeds: agent(FEED_TIMEOUT), packages: agent(PACKAGE_TIMEOUT), retry_after }
    }

    fn signed_feed(&self, name: &str) -> Result<VelopackAssetFeed, FeedError> {
        let feed = self.fetch(name, FEED_LIMIT_BYTES)?;
        let signature = self.fetch(&format!("{name}.sig"), SIGNATURE_BYTES)?;
        verified(&feed, &signature, &self.key)
    }

    /// The file, which is an error if it's longer than `limit` once decompressed, rather than cut to fit.
    fn fetch(&self, name: &str, limit: u64) -> Result<Vec<u8>, FeedError> {
        let url = format!("{}/{name}", self.base);
        let response = call(&self.feeds, &url)?;
        let mut body = Vec::new();
        let read = response.into_body().into_reader().take(limit + 1).read_to_end(&mut body);
        read.map_err(|source| FeedError::Read { url: url.clone(), source })?;
        if body.len() as u64 > limit {
            return Err(FeedError::TooLong { url, limit });
        }
        Ok(body)
    }

    /// Writes the package to `path`, refusing anything longer than the feed says it is once decompressed.
    fn download(&self, asset: &VelopackAsset, path: &Path) -> Result<(), FeedError> {
        let url = format!("{}/{}", self.base, asset.FileName);
        let mut body = call(&self.packages, &url)?.into_body().into_reader().take(asset.Size + 1);
        let saving = |source| FeedError::Save { path: path.to_path_buf(), source };
        let mut file = File::create(path).map_err(saving)?;
        let (mut buffer, mut got) = (vec![0; 1 << 16], 0);
        loop {
            let read = body.read(&mut buffer).map_err(|source| FeedError::Read { url: url.clone(), source })?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(saving)?;
            got += read as u64;
        }
        if got > asset.Size {
            return Err(FeedError::TooLong { url, limit: asset.Size });
        }
        if got < asset.Size {
            return Err(FeedError::CutShort { url, expected: asset.Size, got });
        }
        Ok(())
    }
}

/// The response, if it's the file. A redirect isn't followed, so it's an error like any other answer.
fn call(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, FeedError> {
    let response = agent.get(url).call().map_err(|source| FeedError::Fetch { url: url.to_string(), source })?;
    match response.status().as_u16() {
        200 => Ok(response),
        status => Err(FeedError::Status { url: url.to_string(), status }),
    }
}

impl UpdateSource for SignedFeed {
    fn get_release_feed(&self, channel: &str, _: &Manifest, _: &str) -> Result<VelopackAssetFeed, velopack::Error> {
        let name = format!("releases.{channel}.json");
        match self.signed_feed(&name) {
            Err(FeedError::Unsigned) => {
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

#[cfg(test)]
mod tests;
