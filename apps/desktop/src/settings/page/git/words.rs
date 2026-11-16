//! What Git says in words: what each list says of itself, how an added account's login stands, what stops a default
//! being used, what a machine's own login is, what disconnecting asks, and what the hub's refusals mean. How a sign-in
//! goes is in `sign_in_words`.

use client_core::ClientError;
use gpui_kit::App;
use protocol::client::ErrorCode;
use protocol::model::{GhLogin, GhLoginWorks, GithubLoginKind, HeldGithubLogin, ProviderName};

/// What a search finds the page by, besides its name.
pub(super) const ABOUT: &str = "The GitHub accounts agents push and open pull requests as: signing in with GitHub, \
                                tokens, the default account, and the gh login each machine already has.";

/// What Added says of itself under its heading.
pub(super) const ADDED_SAYS: &str = "GitHub accounts you've given the hub. Every machine's git and gh use the default \
                                     for github.com, so agents push and open pull requests as it, wherever they run.";

/// What Added says of itself under its heading on a hub that keeps no GitHub accounts, which uses no default.
pub(super) const ADDED_SAYS_OLDER: &str = "GitHub accounts you've given the hub.";

/// What Found says of itself under its heading.
pub(super) const FOUND_SAYS: &str = "Who gh is logged in as on each machine a workspace has run on since the hub \
                                     started, and whether GitHub took its token then. Agents use a machine's own \
                                     login only while it's the default, and only there. Roundhouse never copies it.";

/// What Added says while the hub holds no GitHub account.
pub(super) const NONE_ADDED: &str =
    "None yet, so agents use each machine's own login. Connect an account to push as it on every machine.";

/// What Found says while the hub has asked no machine.
pub(super) const NONE_FOUND: &str =
    "None yet. The hub asks each machine once, through the first workspace that runs there after it starts.";

/// What a hub that keeps no GitHub accounts is said to be, in place of both lists.
pub(super) const OLDER_HUB: &str = "This hub is from before hubs kept GitHub accounts, so agents use each machine's own \
                                    login, and it can't connect an account until it's updated.";

/// What's said in place of a row's line when the page was to change something with no hub to ask.
pub(super) const NO_HUB: &str = "There's no hub to ask.";

/// How a login the hub holds, or held, for an account stands, as its row says, and what stops the default being used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Standing {
    /// It works, and ends on the day said, if its row has to say: a token's, since nothing renews it. A sign-in renews
    /// itself, so when its token ends isn't said.
    Works { ends: Option<String> },
    /// A token that ended on the day said, so it needs a new one.
    Ended { on: String },
    /// A sign-in whose token has ended, which the hub is renewing, as it tries to every minute.
    Renewing,
    /// A sign-in that can't be renewed, so it needs signing in again.
    SignInAgain,
    /// The hub holds no login for it.
    Disconnected,
}

impl Standing {
    /// How `held`, the login the hub holds for an account, if any, stands at `now_ms`, with days in the local time zone.
    pub(super) fn of(held: Option<&HeldGithubLogin>, now_ms: i64, cx: &App) -> Standing {
        let Some(held) = held else { return Standing::Disconnected };
        if held.needs_sign_in {
            return Standing::SignInAgain;
        }
        let on = held.ends_at_ms.and_then(|ends_at_ms| look::date(ends_at_ms, cx));
        if held.works_at(now_ms) {
            let ends = (held.how != GithubLoginKind::SignIn).then_some(on).flatten();
            return Standing::Works { ends: ends.map(|on| format!("Ends {on}")) };
        }
        match held.how {
            GithubLoginKind::SignIn => Standing::Renewing,
            GithubLoginKind::Token | GithubLoginKind::Unknown => Standing::Ended { on: on.unwrap_or_default() },
        }
    }

    /// What the row says of it, after how it was connected, if anything.
    pub(super) fn said(&self) -> Option<String> {
        match self {
            Standing::Works { ends } => ends.clone(),
            Standing::Ended { on } => Some(format!("Ended {on}")),
            Standing::Renewing => Some("Renewing".to_string()),
            Standing::SignInAgain => Some("Needs signing in again".to_string()),
            Standing::Disconnected => Some("Disconnected".to_string()),
        }
    }

    /// Whether the login can be used, so the account can be the default.
    pub(super) fn works(&self) -> bool {
        matches!(self, Standing::Works { .. })
    }

    /// What stops the default, called `name`, from being used, and what to do, if it can't be.
    pub(super) fn blocked(&self, name: &str) -> Option<String> {
        let what = match self {
            Standing::Works { .. } => return None,
            Standing::Ended { on } => format!("The token of the default, {name}, ended {on}. Renew it from its menu"),
            Standing::Renewing => {
                return Some(format!(
                    "The token of the default, {name}, has ended, and Roundhouse is renewing it, as it tries to every \
                     minute. If this goes on, sign in again from its menu, or make another account the default."
                ));
            }
            Standing::SignInAgain => {
                format!("The default, {name}, needs signing in with GitHub again. Sign in again from its menu")
            }
            Standing::Disconnected => format!("The default, {name}, was disconnected. Connect it again from its menu"),
        };
        Some(format!("{what}, or make another account the default."))
    }
}

/// How an account's login was connected, as its row says.
pub(super) fn connected_by(how: GithubLoginKind) -> &'static str {
    match how {
        GithubLoginKind::SignIn => "Signed in",
        GithubLoginKind::Token => "Token",
        // A way a newer hub connects one.
        GithubLoginKind::Unknown => "Another way",
    }
}

/// What a machine's own login is called on its row, and what its row says of it, if anything, and whether that's a
/// problem, so it's said in red.
pub(super) fn found(login: &GhLogin) -> (String, Option<(&'static str, bool)>) {
    match login {
        GhLogin::LoggedIn { username, works } => {
            let works = match works {
                GhLoginWorks::Yes => Some(("Works", false)),
                GhLoginWorks::Refused => Some(("GitHub refused its token", true)),
                GhLoginWorks::CouldntTell => Some(("Couldn't tell if it works", false)),
                GhLoginWorks::Unknown => None,
            };
            (username.to_string(), works)
        }
        GhLogin::LoggedOut => ("Not logged in".to_string(), Some(("Run gh auth login there", false))),
        GhLogin::NotInstalled => ("No gh".to_string(), Some(("Install gh there", false))),
        GhLogin::Unknown => ("Unknown".to_string(), None),
    }
}

/// Whose `gh` a machine's own login is, by the provider whose machines it's on.
pub(super) fn found_on(provider: &ProviderName) -> String {
    if provider.is_local() {
        return format!("gh on this {}", crate::names::MACHINE);
    }
    format!("gh on {provider}")
}

/// What disconnecting the account called `name` asks first.
pub(super) fn disconnect_asks(name: &str) -> String {
    format!("Disconnect {name}?")
}

/// What the question before disconnecting an account says under it, and more when it's the `default`.
pub(super) fn disconnect_says(default: bool) -> &'static str {
    if default {
        "The hub forgets its login, so connecting it again takes a new token or signing in again. It isn't revoked on \
         GitHub, so it still works wherever else it's kept. It stays the default, so agents can't push until you \
         connect it again or make another account the default."
    } else {
        "The hub forgets its login, so connecting it again takes a new token or signing in again. It isn't revoked on \
         GitHub, so it still works wherever else it's kept."
    }
}

/// What to say when the hub couldn't do `what` for an account, such as "disconnect it".
pub(super) fn couldnt(what: &str, e: &ClientError) -> String {
    if e.is_not_found() {
        return "The hub no longer has this account.".to_string();
    }
    format!("Couldn't {what}: {}", reason(e))
}

/// Why the hub refused, as the hub says it for a person, or as the app says what went wrong on the way.
pub(super) fn reason(e: &ClientError) -> String {
    let ClientError::Api(api) = e else { return e.to_string() };
    match api.code {
        ErrorCode::Unsupported => "this hub can't do that until it's updated".to_string(),
        ErrorCode::InvalidRequest
        | ErrorCode::VersionMismatch
        | ErrorCode::NeedsForce
        | ErrorCode::NameTaken
        | ErrorCode::NotFound
        | ErrorCode::Conflict
        | ErrorCode::Worker
        | ErrorCode::Unavailable
        | ErrorCode::Internal
        | ErrorCode::Unknown => api.message.clone(),
    }
}
