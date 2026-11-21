//! Where the release workflow publishes the feed, and the key it signs it with.

use super::UpdateFeedKey;

/// Where updates come from. The release workflow publishes to the URL in `update-feed.url`. A build can name another
/// in `ROUNDHOUSE_UPDATE_URL`, such as a server on this machine.
pub const URL: &str = match option_env!("ROUNDHOUSE_UPDATE_URL") {
    Some(url) => url,
    None => include_str!("../../update-feed.url").trim_ascii(),
};

/// The update feed's public key. The release workflow signs with the private half of the one in `update-feed.key`. A
/// build can name another in `ROUNDHOUSE_UPDATE_KEY`, such as one made for a test.
pub const KEY: UpdateFeedKey = UpdateFeedKey::from_base64(match option_env!("ROUNDHOUSE_UPDATE_KEY") {
    Some(key) => key,
    None => include_str!("../../update-feed.key").trim_ascii(),
});

/// Which platform's feed this build's hub reads, as `tools/package/publish.py` names it. `None` where no hub is
/// published, as on Windows, whose hub isn't installed as a service.
pub const HUB_CHANNEL: Option<&str> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("osx")
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-amd64")
} else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
    Some("linux-arm64")
} else {
    None
};
