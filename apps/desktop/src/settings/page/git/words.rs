//! What Git says in words: what each list says of itself, how an added account's login stands, what a machine's own
//! login is, what disconnecting asks, and what the hub not doing what was asked of an account means, with why it
//! refused as client-core's `refusals` says it. What stops a default being used is the hub's to say, in its own words.
//! How a sign-in goes is in `sign_in_words`, and what's said of apps of your own in `app_words`.

use client_core::{ClientError, refusals};
use gpui_kit::App;
use protocol::model::{
    GhLogin, GhLoginWorks, GithubLoginKind, HeldGithubLogin, ProviderName, SentCredential, UnusableLogin,
};

/// What a search finds the page by, besides its name.
pub(super) const ABOUT: &str = "The GitHub accounts agents push and open pull requests as: signing in with GitHub, \
                                tokens, apps of your own, the default account, and the gh login each machine already \
                                has.";

/// What Added says of itself under its heading.
pub(super) const ADDED_SAYS: &str = "GitHub accounts you've given the hub. Agents push and open pull requests as the \
                                     default on the providers Roundhouse sets up GitHub on. On the hub's own machine, \
                                     they use its git and gh as they are.";

/// What Added says of itself under its heading on a hub that keeps no GitHub accounts, which uses no default.
pub(super) const ADDED_SAYS_OLDER: &str = "GitHub accounts you've given the hub.";

/// What Found says of itself under its heading.
pub(super) const FOUND_SAYS: &str = "Who gh is logged in as on each machine a workspace has run on since the hub \
                                     started, and whether GitHub took its token then. Agents on the hub's own \
                                     machine use its login, and elsewhere only while it's the default. Roundhouse \
                                     never copies it.";

/// What Added says while the hub holds no GitHub account.
pub(super) const NONE_ADDED: &str = "None yet, so agents use each machine's own login. Connect an account to push as it on the providers Roundhouse \
     sets up GitHub on.";

/// What Found says while the hub has asked no machine.
pub(super) const NONE_FOUND: &str =
    "None yet. The hub asks each machine once, through the first workspace that runs there after it starts.";

/// What a hub that keeps no GitHub accounts is said to be, in place of both lists.
pub(super) const OLDER_HUB: &str = "This hub is from before hubs kept GitHub accounts, so agents use each machine's own \
                                    login, and it can't connect an account until it's updated.";

/// What's said in place of a row's line when the page was to change something with no hub to ask.
pub(super) const NO_HUB: &str = "There's no hub to ask.";

/// How a login the hub holds, or held, for an account stands, as its row says: the default's as the hub says what
/// workers are sent for it, and any other's as its record says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Standing {
    /// It works, and ends on the day said, if its row has to say: a token's, since nothing renews it. A sign-in renews
    /// itself, and the hub makes an installation's next token itself, so when either's ends isn't said.
    Works { ends: Option<String> },
    /// A token that ended on the day said, if its record says one, so it needs a new one.
    Ended { on: String },
    /// A sign-in whose token has ended, which the hub is renewing, as it tries to every minute, or, as the hub says it
    /// of the default, an installation's, which it's making the next of.
    Renewing,
    /// A sign-in that can't be renewed, so it needs signing in again.
    SignInAgain,
    /// The hub holds no login for it.
    Disconnected,
    /// The hub can't use it for a reason no record says, as it says of the default: a login only a newer Roundhouse
    /// reads, one it can't read, or a reason from a newer hub. What the hub says of it is above the rows.
    CantBeUsed,
}

impl Standing {
    /// How `held`, the login the hub holds for an account, if any, stands at `now_ms`, as its record says, with days in
    /// the local time zone. The record of an installation says nothing of when its token ends, so it works for as long
    /// as the hub holds it, as far as the record says.
    pub(super) fn of(held: Option<&HeldGithubLogin>, now_ms: i64, cx: &App) -> Standing {
        let Some(held) = held else { return Standing::Disconnected };
        if held.needs_sign_in {
            return Standing::SignInAgain;
        }
        if held.works_at(now_ms) {
            return Standing::Works { ends: ends(held, cx) };
        }
        match held.how {
            GithubLoginKind::SignIn => Standing::Renewing,
            GithubLoginKind::Token | GithubLoginKind::App | GithubLoginKind::Unknown => {
                Standing::Ended { on: ended_on(held, cx) }
            }
        }
    }

    /// How the default's login stands, as `sent`, what the hub sends every worker for it, says, with `held`, what its
    /// record says of its login, for when a token ends, which only the record says.
    pub(super) fn sent(sent: &SentCredential, held: Option<&HeldGithubLogin>, cx: &App) -> Standing {
        let why = match sent {
            SentCredential::Account { .. } => {
                return Standing::Works { ends: held.and_then(|held| ends(held, cx)) };
            }
            SentCredential::Blocked { why, .. } => why,
            SentCredential::MachineOwn | SentCredential::Unknown => return Standing::CantBeUsed,
        };
        match why {
            UnusableLogin::Disconnected => Standing::Disconnected,
            UnusableLogin::Ended => Standing::Ended { on: held.map_or_default(|held| ended_on(held, cx)) },
            UnusableLogin::Renewing => Standing::Renewing,
            UnusableLogin::SignInAgain => Standing::SignInAgain,
            UnusableLogin::Newer | UnusableLogin::Unreadable | UnusableLogin::Unknown => Standing::CantBeUsed,
        }
    }

    /// What the row says of it, after how it was connected, if anything.
    pub(super) fn said(&self) -> Option<String> {
        match self {
            Standing::Works { ends } => ends.clone(),
            Standing::Ended { on } if on.is_empty() => Some("Ended".to_string()),
            Standing::Ended { on } => Some(format!("Ended {on}")),
            Standing::Renewing => Some("Renewing".to_string()),
            Standing::SignInAgain => Some("Needs signing in again".to_string()),
            Standing::Disconnected => Some("Disconnected".to_string()),
            Standing::CantBeUsed => Some("Can't be used".to_string()),
        }
    }

    /// Whether the login can be used, so the account can be the default.
    pub(super) fn works(&self) -> bool {
        matches!(self, Standing::Works { .. })
    }
}

/// What a row says of when `held` ends while it works: a token's day, as `Ends Feb 3, 2027`, since nothing renews it,
/// and nothing for a sign-in's or an installation's, which the hub makes again itself, or one that doesn't end.
fn ends(held: &HeldGithubLogin, cx: &App) -> Option<String> {
    let renewed = matches!(held.how, GithubLoginKind::SignIn | GithubLoginKind::App);
    let on = held.ends_at_ms.filter(|_| !renewed).and_then(|ends_at_ms| look::date(ends_at_ms, cx));
    on.map(|on| format!("Ends {on}"))
}

/// The day `held` ended, in the local time zone, or nothing when its record says no end.
fn ended_on(held: &HeldGithubLogin, cx: &App) -> String {
    held.ends_at_ms.and_then(|ends_at_ms| look::date(ends_at_ms, cx)).unwrap_or_default()
}

/// How an account's login was connected, as its row says.
pub(super) fn connected_by(how: GithubLoginKind) -> &'static str {
    match how {
        GithubLoginKind::SignIn => "Signed in",
        GithubLoginKind::Token => "Token",
        GithubLoginKind::App => super::app_words::APP,
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

/// What to say when the hub couldn't do `what` for an account, such as "disconnect it", or no longer has it.
pub(super) fn couldnt(what: &str, e: &ClientError) -> String {
    if e.is_not_found() {
        return refusals::gone("account");
    }
    refusals::couldnt(what, e)
}
