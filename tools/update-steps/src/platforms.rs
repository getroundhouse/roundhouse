use std::time::Duration;

use anyhow::Context as _;
use fetching::{FetchError, Fetcher};
use protocol::versions::Versions;
use update_feed::Feed;

/// Where apps read the feeds, as the app has it.
const UPDATE_URL: &str = include_str!("../../../crates/update-feed/update-feed.url");

/// A platform whose apps step, by the channel it publishes to, as `publish.py` names it.
struct Stepping {
    channel: &'static str,
    /// The channel it took over from, as `TAKES_OVER` in `publish.py` has it, if it did.
    took_over: Option<&'static str>,
}

/// The platforms whose apps step: those whose hubs keep their workers through an update, as `KEEPS_WORKERS` in the hub
/// has it, and as `STEPS` in `tools/package/publish.py` has them. A Windows hub's workers stop with it, so its apps
/// take the newest.
const STEPPING: [Stepping; 3] = [
    Stepping { channel: "macos-arm64", took_over: Some("osx") },
    Stepping { channel: "linux-amd64", took_over: None },
    Stepping { channel: "linux-arm64", took_over: None },
];

/// As long as the app takes a feed to be, once decompressed, as `FEED_LIMIT_BYTES` in its `updates::source` has it.
const FEED_LIMIT_BYTES: u64 = 1024 * 1024;

/// Reads each stepping platform's feed, and fails, saying why, unless a hub that speaks `speaks` talks to the workers
/// of each one's newest release. The feed's signature isn't checked, since this only holds a change back, and what's
/// published still has to check it.
pub(crate) fn leave_a_step(speaks: Versions) -> anyhow::Result<()> {
    let fetcher = Fetcher::new(Duration::from_secs(60));
    let mut stranded = Vec::new();
    for platform in &STEPPING {
        let feed = newest_feed(&fetcher, UPDATE_URL.trim_ascii(), platform)?;
        if let Some(release) = feed.stranded_by(speaks)? {
            let its = release.worker_versions.map_or(0, |versions| versions.newest);
            stranded.push(format!("{}: {}'s hub runs workers of protocol {its}", platform.channel, release.version));
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
    let channels: Vec<_> = STEPPING.iter().map(|platform| platform.channel).collect();
    println!("A hub that speaks workers of {speaks} leaves every app on {} a release to step to.", channels.join(", "));
    Ok(())
}

/// The feed of `platform`'s newest releases. A channel that took over from another has no feed until its first release
/// publishes, and until then the older channel's has them, so a missing feed is read there, and only for such a
/// channel. Every other failure fails.
fn newest_feed(fetcher: &Fetcher, base: &str, platform: &Stepping) -> anyhow::Result<Feed> {
    let read = |channel: &str| -> Result<(String, Vec<u8>), FetchError> {
        let url = format!("{base}/releases.{channel}.json");
        let bytes = fetcher.bytes(&url, FEED_LIMIT_BYTES)?;
        Ok((url, bytes))
    };
    let (url, bytes) = match (read(platform.channel), platform.took_over) {
        (Err(FetchError::Status { status: 404, .. }), Some(older)) => read(older)?,
        (read, _) => read?,
    };
    serde_json::from_slice(&bytes).with_context(|| format!("reading {url}"))
}

#[cfg(test)]
mod tests {
    use fetching::serving::{Reply, serve};

    use super::*;

    const FEED: &str = r#"{"Assets":[{"Version":"0.1.2","Type":"Full","WorkerVersions":{"oldest":4,"newest":5}}]}"#;

    fn versions(base: &str, platform: &Stepping) -> anyhow::Result<Vec<String>> {
        let feed = newest_feed(&Fetcher::new(Duration::from_secs(5)), base, platform)?;
        Ok(feed.releases().map(|release| release.version.to_owned()).collect())
    }

    /// Until a channel that took over from another publishes, the older one's feed has the newest releases.
    #[test]
    fn a_channel_that_took_over_is_read_where_it_took_over_from_until_it_publishes() {
        let renamed = Stepping { channel: "new", took_over: Some("old") };
        let never = Stepping { channel: "new", took_over: None };
        let older = serve(vec![("releases.old.json".to_owned(), vec![Reply::file(FEED)])]).unwrap();
        let both = serve(vec![
            ("releases.old.json".to_owned(), vec![Reply::file(FEED.replace("0.1.2", "0.1.1"))]),
            ("releases.new.json".to_owned(), vec![Reply::file(FEED)]),
        ])
        .unwrap();

        assert_eq!(versions(&older.base, &renamed).unwrap(), ["0.1.2"]);
        assert_eq!(versions(&both.base, &renamed).unwrap(), ["0.1.2"], "its own, once it has one");
        let missing = versions(&older.base, &never).unwrap_err();
        assert!(matches!(missing.downcast_ref(), Some(FetchError::Status { status: 404, .. })), "{missing:?}");
    }
}
