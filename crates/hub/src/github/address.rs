//! Where the hub calls GitHub's API: GitHub's own, or for a test's hub, a stand-in.

#[cfg(feature = "testing")]
use protocol::paths::ENV_TEST_GITHUB_API;

/// GitHub's own API.
const GITHUB_API: &str = "https://api.github.com";

/// GitHub's API, as `main` gives it to [`HubConfig`](crate::HubConfig), whatever `var` says, since only a build with
/// the `testing` feature takes another, and the product is never built with it.
#[cfg(not(feature = "testing"))]
pub fn github_api(_var: impl Fn(&str) -> Option<String>) -> String {
    GITHUB_API.to_string()
}

/// What `var` gives for [`ENV_TEST_GITHUB_API`], such as a stand-in's address, and otherwise GitHub's own API.
#[cfg(feature = "testing")]
pub fn github_api(var: impl Fn(&str) -> Option<String>) -> String {
    var(ENV_TEST_GITHUB_API).unwrap_or_else(|| GITHUB_API.to_string())
}
