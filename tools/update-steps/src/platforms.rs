use std::time::Duration;

use anyhow::Context as _;
use fetching::Fetcher;
use protocol::versions::Versions;
use update_feed::Feed;

/// Where apps read the feeds, as the app has it.
const UPDATE_URL: &str = include_str!("../../../crates/update-feed/update-feed.url");

/// The platforms whose apps step: those whose hubs keep their workers through an update, as `KEEPS_WORKERS` in the hub
/// has it, and as `STEPS` in `tools/package/publish.py` has them. A Windows hub's workers stop with it, so its apps
/// take the newest.
const STEPPING: [&str; 3] = ["osx", "linux-amd64", "linux-arm64"];

/// As long as the app takes a feed to be, once decompressed, as `FEED_LIMIT_BYTES` in its `updates::source` has it.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;

/// Reads each stepping platform's feed, and fails, saying why, unless a hub that speaks `speaks` talks to the workers
/// of each one's newest release. The feed's signature isn't checked, since this only holds a change back, and what's
/// published still has to check it.
pub(crate) fn leave_a_step(speaks: Versions) -> anyhow::Result<()> {
    let fetcher = Fetcher::new(Duration::from_secs(60));
    let mut stranded = Vec::new();
    for platform in STEPPING {
        let url = format!("{}/releases.{platform}.json", UPDATE_URL.trim_ascii());
        let feed = fetcher.bytes(&url, FEED_LIMIT_BYTES)?;
        let feed: Feed = serde_json::from_slice(&feed).with_context(|| format!("reading {url}"))?;
        if let Some(release) = feed.stranded_by(speaks)? {
            let its = release.worker_versions.map_or(0, |versions| versions.newest);
            stranded.push(format!("{platform}: {}'s hub runs workers of protocol {its}", release.version));
        }
    }
    anyhow::ensure!(
        stranded.is_empty(),
        "This hub speaks workers of {speaks}, so an app with these releases' workers at work would have no release to \
         step to:\n{}\nA break in the worker protocol merges only once each platform whose apps step has published a \
         release with the one before, as \"A break in the worker protocol\" in docs/standards/merging.md says. Queue \
         it again once they have.",
        stranded.join("\n")
    );
    println!("A hub that speaks workers of {speaks} leaves every app on {} a release to step to.", STEPPING.join(", "));
    Ok(())
}
