//! Where the app gets updates: the update feed, trusted only once its signature matches the key built into the app, as
//! `update_feed` checks it, and the packages it names.
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

mod keeping;
mod signed_feed;
mod stepping;

pub(crate) use keeping::{Keeping, Kept, ToKeep};
#[cfg(test)]
use signed_feed::FEED_LIMIT_BYTES;
#[cfg(test)]
pub(crate) use signed_feed::verified;
pub(crate) use signed_feed::{SignedSource, SourceError};
use stepping::only_taking_over;
