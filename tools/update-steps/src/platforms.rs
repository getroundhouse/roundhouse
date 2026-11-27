use std::time::Duration;

use anyhow::Context as _;
use fetching::Fetcher;
use update_feed::{Feed, Floors, Stranded};

/// Where apps read the feeds, as the app has it.
const UPDATE_URL: &str = include_str!("../../../crates/update-feed/update-feed.url");

/// Every platform, by the channel it publishes to, as `publish.py` names it, since every app takes only a release that
/// updates from its own, and every platform's hubs keep their workers through an update, so its apps step through each
/// break in the worker protocol.
const CHANNELS: [&str; 5] = ["macos-arm64", "linux-amd64", "linux-arm64", "win-amd64", "win-arm64"];

/// As long as the app takes a feed to be, once decompressed, as `FEED_LIMIT_BYTES` in its `updates::source` has it.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;

/// Reads each platform's feed, and fails, saying why, unless a build with `floors` leaves an app on each one's newest
/// release a release to step to, as [`Feed::stranded_by`] says. The feed's signature isn't checked, since this only
/// holds a change back, and what's published still has to check it.
pub(crate) fn leave_a_step(floors: Floors) -> anyhow::Result<()> {
    let fetcher = Fetcher::new(Duration::from_secs(60));
    let mut stranded = Vec::new();
    for channel in CHANNELS {
        let url = format!("{}/releases.{channel}.json", UPDATE_URL.trim_ascii());
        let feed = fetcher.bytes(&url, FEED_LIMIT_BYTES)?;
        let feed: Feed = serde_json::from_slice(&feed).with_context(|| format!("reading {url}"))?;
        if let Some(why) = feed.stranded_by(floors)? {
            stranded.push(format!("{channel}: {}", said(why, floors)));
        }
    }
    anyhow::ensure!(
        stranded.is_empty(),
        "This build would leave an app with no release to step to:\n{}\nA break in the worker protocol merges only once \
         each platform has published a release with the one before, and a build updates only from a \
         release each platform keeps for apps to step to, as \"A break in the worker protocol\" in \
         docs/standards/merging.md says. Queue it again once they have.",
        stranded.join("\n")
    );
    println!("A build with these floors leaves every app on {} a release to step to.", CHANNELS.join(", "));
    Ok(())
}

/// What `why` means for a build with `floors`, in a line.
fn said(why: Stranded<'_>, floors: Floors) -> String {
    let (speaks, from) = (floors.workers, floors.release.unwrap_or("any"));
    match why {
        Stranded::Workers(newest) => {
            let its = newest.worker_versions.map_or(0, |versions| versions.newest);
            format!("{}'s hub runs workers of protocol {its}, and this one speaks {speaks}", newest.version)
        }
        Stranded::Release(newest) => format!("the newest release is {}, older than {from}", newest.version),
        Stranded::NotKept => format!("{from} isn't a release the feed keeps for apps to step to"),
        Stranded::PastTheWorkerStep(Some(last)) => format!(
            "{from} is newer than {}, the last release whose hub doesn't speak workers of protocol {}",
            last.version, speaks.oldest
        ),
        Stranded::PastTheWorkerStep(None) => {
            format!(
                "no release in the feed is from before workers of protocol {}, so none can be a floor",
                speaks.oldest
            )
        }
    }
}
