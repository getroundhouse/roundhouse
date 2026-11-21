use std::collections::HashSet;

use update_feed::Feed;
use velopack::VelopackAssetFeed;

/// `releases` without each one whose hub doesn't speak workers of `oldest`, as `feed` says, so Velopack takes the
/// newest that does, and an update crosses at most one break in the worker protocol. Every worker to keep speaks that
/// version or a newer one, up to the running hub's own, which a newer release's hub speaks too, as long as no published
/// break in the worker protocol is reverted, as `docs/standards/merging.md` says. A release from before releases said
/// what they speak goes too, since its hub may not.
pub(crate) fn only_speaking(mut releases: VelopackAssetFeed, feed: &Feed, oldest: u32) -> VelopackAssetFeed {
    let speaking: HashSet<&str> = feed.speaking(oldest).map(|release| release.version).collect();
    releases.Assets.retain(|package| speaking.contains(package.Version.as_str()));
    releases
}
