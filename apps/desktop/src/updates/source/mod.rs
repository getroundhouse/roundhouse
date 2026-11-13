//! Where the app gets updates: the update feed, trusted only once its signature matches the key built into the app,
//! and the packages it names.
//!
//! This is the one definition of the signature, which `tools/package/feed-signature.sh` makes and the release workflow
//! checks. It's Ed25519 over the feed's exact bytes, as served, and the raw 64 bytes of it are beside the feed, in
//! `releases.<channel>.json.sig`. The public key is the raw 32 bytes, in base64. `source/fixture/` holds a feed that
//! OpenSSL signed with a throwaway key, which the tests here and `tools/package/test_publish.py` both check.
//!
//! Velopack checks a package's size and checksum against the feed once it has downloaded it, so a changed package is
//! refused, and nothing longer than the feed says is downloaded at all. Velopack installs whatever is at the package's
//! path when the app quits, without checking it again, so something running as this user could swap it, but it could
//! as well change the app itself, which this user owns.

#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use velopack::bundle::Manifest;
#[cfg(test)]
use velopack::sources::UpdateSource;
#[cfg(test)]
use velopack::{VelopackAsset, VelopackAssetFeed};

#[cfg(test)]
mod tests;

mod signed_feed;

#[cfg(test)]
use signed_feed::FEED_LIMIT_BYTES;
#[cfg(test)]
pub(crate) use signed_feed::verified;
pub(crate) use signed_feed::{SignedSource, SourceError, UpdateFeedKey};
