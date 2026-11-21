//! Where the hub calls GitHub, and the OAuth App it signs people in with GitHub through: GitHub's own, or for a test's
//! hub, a stand-in.

#[cfg(any(test, feature = "testing"))]
use protocol::paths::ENV_TEST_GITHUB_API;

/// GitHub's own API.
const GITHUB_API: &str = "https://api.github.com";

/// Where GitHub's own device flow is.
const GITHUB_LOGIN: &str = "https://github.com";

/// Where GitHub serves its own repositories over HTTPS.
const GITHUB_GIT: &str = "https://github.com";

/// Roundhouse's OAuth App on GitHub, which every hub has people sign in with GitHub through. The device flow needs no
/// client secret, so the ID is all a hub has of the app, and it's no secret.
const ROUNDHOUSE_APP: &str = "Ov23liWtqywYN3M2m6EU";

/// Where the hub calls GitHub, and the OAuth App people sign in with GitHub through, as `main` gives them to the hub's
/// `HubConfig` from [`GithubConfig::from_env`]. Given to the hub rather than known to its calls, so a hub can be pointed
/// at a stand-in, as the hub's own tests point theirs.
#[derive(Clone, Debug)]
pub struct GithubConfig {
    /// GitHub's API, such as `https://api.github.com`, which the hub asks whose a token is.
    pub(crate) api: String,
    /// Where GitHub's device flow is, such as `https://github.com`, which the hub asks for a sign-in's code and token.
    pub(crate) login: String,
    /// Where GitHub serves repositories over HTTPS, such as `https://github.com`, which the hub asks whether a push
    /// would be taken, as `git push` asks it.
    pub(crate) git: String,
    /// The OAuth App a sign-in is through, which is what GitHub shows the person as they approve.
    pub(crate) client_id: GithubClientId,
}

/// The client ID of an OAuth App on GitHub, such as `Ov23liWtqywYN3M2m6EU`. Every hub that signs in through the app
/// sends the same, and it's no secret: the device flow needs no client secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GithubClientId(String);

impl GithubClientId {
    pub(crate) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl GithubConfig {
    /// GitHub's own, through Roundhouse's app, whatever `var` says, since only a build with the `testing` feature takes
    /// another, and the product is never built with it.
    #[cfg(not(any(test, feature = "testing")))]
    pub fn from_env(_var: impl Fn(&str) -> Option<String>) -> Self {
        Self::of(GITHUB_API.to_string(), GITHUB_LOGIN.to_string(), GITHUB_GIT.to_string())
    }

    /// GitHub's API, its device flow and its repositories all at what `var` gives for [`ENV_TEST_GITHUB_API`], such as
    /// a stand-in's address, which serves them all, and otherwise GitHub's own, through Roundhouse's app.
    #[cfg(any(test, feature = "testing"))]
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        match var(ENV_TEST_GITHUB_API) {
            Some(address) => Self::of(address.clone(), address.clone(), address),
            None => Self::of(GITHUB_API.to_string(), GITHUB_LOGIN.to_string(), GITHUB_GIT.to_string()),
        }
    }

    fn of(api: String, login: String, git: String) -> Self {
        Self { api, login, git, client_id: GithubClientId::new(ROUNDHOUSE_APP) }
    }

    /// A GitHub whose API, device flow and repositories are all at `address`, such as a stand-in's, signed in through an
    /// app of a made-up ID.
    #[cfg(any(test, feature = "testing"))]
    pub fn at(address: String) -> Self {
        let client_id = GithubClientId::new("a-made-up-client-id");
        Self { api: address.clone(), login: address.clone(), git: address, client_id }
    }
}
