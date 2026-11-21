use std::time::Duration;

use anyhow::Context as _;
use fetching::Fetcher;
use update_feed::{Feed, Floors, Stranded};

/// Where apps read the feeds, as the app has it.
const UPDATE_URL: &str = include_str!("../../../crates/update-feed/update-feed.url");

/// A platform, by the channel it publishes to, as `publish.py` names it.
struct Platform {
    channel: &'static str,
    /// Whether its apps step through each break in the worker protocol: whether its hubs keep their workers through an
    /// update, as `KEEPS_WORKERS` in the hub has it, and as `STEPS` in `tools/package/publish.py` has them. A Windows
    /// hub's workers stop with it, so its apps keep none.
    keeps_workers: bool,
}

/// Every platform, since every app takes only a release that updates from its own.
const PLATFORMS: [Platform; 5] = [
    Platform { channel: "macos-arm64", keeps_workers: true },
    Platform { channel: "linux-amd64", keeps_workers: true },
    Platform { channel: "linux-arm64", keeps_workers: true },
    Platform { channel: "win-amd64", keeps_workers: false },
    Platform { channel: "win-arm64", keeps_workers: false },
];

/// As long as the app takes a feed to be, once decompressed, as `FEED_LIMIT_BYTES` in its `updates::source` has it.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;

/// Reads each platform's feed, and fails, saying why, unless a build with `floors` leaves an app on each one's newest
/// release a release to step to, as [`Feed::stranded_by`] says. Only the platforms whose apps keep workers have
/// anything to check while the build says no oldest release it updates from, so only those feeds are read then. The
/// feed's signature isn't checked, since this only holds a change back, and what's published still has to check it.
pub(crate) fn leave_a_step(floors: Floors) -> anyhow::Result<()> {
    let fetcher = Fetcher::new(Duration::from_secs(60));
    let mut stranded = Vec::new();
    let checked: Vec<_> =
        PLATFORMS.iter().filter(|platform| platform.keeps_workers || floors.release.is_some()).collect();
    for platform in &checked {
        let url = format!("{}/releases.{}.json", UPDATE_URL.trim_ascii(), platform.channel);
        let feed = fetcher.bytes(&url, FEED_LIMIT_BYTES)?;
        let feed: Feed = serde_json::from_slice(&feed).with_context(|| format!("reading {url}"))?;
        if let Some(why) = feed.stranded_by(floors, platform.keeps_workers)? {
            stranded.push(format!("{}: {}", platform.channel, said(why, floors)));
        }
    }
    anyhow::ensure!(
        stranded.is_empty(),
        "This build would leave an app with no release to step to:\n{}\nA break in the worker protocol merges only once \
         each platform whose apps step has published a release with the one before, and a build updates only from a \
         release each platform keeps for apps to step to, as \"A break in the worker protocol\" in \
         docs/standards/merging.md says. Queue it again once they have.",
        stranded.join("\n")
    );
    let channels: Vec<_> = checked.iter().map(|platform| platform.channel).collect();
    println!("A build with these floors leaves every app on {} a release to step to.", channels.join(", "));
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
