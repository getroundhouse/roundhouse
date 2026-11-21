use std::collections::HashSet;

use update_feed::{Feed, Running};
use velopack::VelopackAssetFeed;

/// `releases` without each one an update from `running` can't take, as `feed` says, so Velopack takes the newest it
/// can. That's past both of the release's floors: its hub speaks the workers to keep, so the update crosses at most one
/// break in the worker protocol, and it updates from `running`'s release, so its hub can migrate the database. Every
/// worker to keep speaks that version or a newer one, up to the running hub's own, which a newer release's hub speaks
/// too, as long as no published break in the worker protocol is reverted, as `docs/standards/merging.md` says. A release
/// from before releases said what they speak goes too, while there are workers to keep, since its hub may not.
pub(crate) fn only_taking_over(
    mut releases: VelopackAssetFeed,
    feed: &Feed,
    running: Running<'_>,
) -> VelopackAssetFeed {
    let taken: HashSet<&str> = feed.taking_over(running).map(|release| release.version).collect();
    releases.Assets.retain(|package| taken.contains(package.Version.as_str()));
    releases
}
