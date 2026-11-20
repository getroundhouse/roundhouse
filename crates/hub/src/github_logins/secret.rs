use std::io;

use protocol::model::GithubToken;
use serde::{Deserialize, Serialize};

use crate::login_files::{LoginSecret, Stored};
use crate::store::later_ms;
use github::{GithubAppKey, GithubRefreshToken, Granted};

/// The login the hub holds for a GitHub account, or the key of an app of your own, in a file of its own in the GitHub
/// logins' directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubLogin {
    /// A token the person made on GitHub and gave a client.
    Token(GithubToken),
    /// What signing in with GitHub gave.
    SignIn(SignedIn),
    /// An installation's token, as the hub made it from its app's key.
    Installation(InstallationToken),
    /// An app of your own's key, which is no account's login, as the app's row names it.
    App(GithubAppKey),
}

impl GithubLogin {
    /// The token that works as the account, whichever way it was connected, and `None` for an app's key, which isn't
    /// an account's.
    pub(crate) fn token(&self) -> Option<&GithubToken> {
        match self {
            GithubLogin::Token(token) => Some(token),
            GithubLogin::SignIn(signed_in) => Some(&signed_in.token),
            GithubLogin::Installation(installation) => Some(&installation.token),
            GithubLogin::App(_) => None,
        }
    }
}

/// What a sign-in with GitHub gave the hub: a token that works as the account until it ends, and what renews it, until
/// that ends too, each end by the hub's clock, or `None` for one that doesn't end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SignedIn {
    pub(crate) token: GithubToken,
    #[serde(default)]
    pub(crate) token_ends_at_ms: Option<i64>,
    #[serde(default)]
    pub(crate) refresh: Option<GithubRefreshToken>,
    #[serde(default)]
    pub(crate) refresh_ends_at_ms: Option<i64>,
}

impl SignedIn {
    /// What GitHub `granted` at `granted_at_ms`, with when each part ends by the hub's clock.
    pub(crate) fn given(granted: Granted, granted_at_ms: i64) -> Self {
        let Granted { token, lasts, refresh } = granted;
        let (refresh, refresh_lasts) = refresh.map_or((None, None), |(refresh, lasts)| (Some(refresh), lasts));
        let ends = |lasts: Option<std::time::Duration>| lasts.map(|lasts| later_ms(granted_at_ms, lasts));
        Self { token, token_ends_at_ms: ends(lasts), refresh, refresh_ends_at_ms: ends(refresh_lasts) }
    }

    /// What renews its token at `now_ms`, by the hub's clock, unless GitHub gave none or it has ended, so only signing
    /// in again gives the account a login that works once the token ends.
    pub(crate) fn renews_with(&self, now_ms: i64) -> Option<&GithubRefreshToken> {
        let ended = self.refresh_ends_at_ms.is_some_and(|ends_at_ms| ends_at_ms <= now_ms);
        self.refresh.as_ref().filter(|_| !ended)
    }
}

/// An installation's token, which the hub made from its app's key, and when it ends, by the hub's clock, an hour after
/// GitHub gave it. Making another takes nothing from it, so the hub makes the next before it ends, for as long as the
/// app is installed there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InstallationToken {
    pub(crate) token: GithubToken,
    pub(crate) token_ends_at_ms: i64,
}

/// How a file that isn't a token's holds what it holds: a JSON object whose `format` says which form it's in. A form
/// only a newer build knows is kept for that build, as [`Stored::Newer`] says, rather than taken for what isn't a login
/// and deleted, as tidying would, so going back to an older build loses no sign-in or app. So a change that only adds
/// a field keeps the form, which an older build reads past what it doesn't know of, and a change it would misread is a
/// new form. Neither needs a migration of the database, which would stop an older build opening the store at all.
#[derive(Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
enum Form {
    SignIn(SignedIn),
    Installation(InstallationToken),
    App {
        key: GithubAppKey,
    },
    #[serde(other)]
    Newer,
}

/// A token is kept as its text, as it always was. Anything else is kept as a JSON object of its [`Form`], which no
/// token's text starts as, so the first byte tells the two apart.
impl LoginSecret for GithubLogin {
    const DIR: &'static str = protocol::paths::GITHUB_LOGINS_DIR;

    fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let form = match self {
            GithubLogin::Token(token) => return Ok(token.as_str().as_bytes().to_vec()),
            GithubLogin::SignIn(signed_in) => Form::SignIn(signed_in.clone()),
            GithubLogin::Installation(installation) => Form::Installation(installation.clone()),
            GithubLogin::App(key) => Form::App { key: key.clone() },
        };
        Ok(serde_json::to_vec(&form)?)
    }

    /// Text that isn't a JSON object is a token. A JSON object that isn't one of the forms it can be, such as one cut
    /// short, holds none.
    fn from_bytes(bytes: Vec<u8>) -> Stored<Self> {
        if bytes.first() == Some(&b'{') {
            return match serde_json::from_slice(&bytes) {
                Ok(Form::SignIn(signed_in)) => Stored::Secret(GithubLogin::SignIn(signed_in)),
                Ok(Form::Installation(installation)) => Stored::Secret(GithubLogin::Installation(installation)),
                Ok(Form::App { key }) => Stored::Secret(GithubLogin::App(key)),
                Ok(Form::Newer) => Stored::Newer,
                Err(_) => Stored::Nothing,
            };
        }
        String::from_utf8(bytes)
            .map_or(Stored::Nothing, |token| Stored::Secret(GithubLogin::Token(GithubToken::new(token))))
    }
}
