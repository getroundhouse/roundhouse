//! How a sign-in with GitHub stands, as its panel shows it, from the hub's record of it, and what the panel says of
//! each: the code to type while it waits, and how it ended.

use client_core::records::Snapshot;
use client_core::{ClientError, refusals};
use gpui_kit::App;
use protocol::client::ErrorCode;
use protocol::model::{GithubSettings, GithubSignInStatus, GithubUserCode};

use super::sign_in::SignInPanel;

/// How the sign-in a panel shows stands.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Stands<'a> {
    /// The hub hasn't answered its start yet.
    Starting,
    /// The hub couldn't start it, as this says.
    NotStarted(&'a str),
    /// It waits for the person to type `code` at `address`, until the time said, if it can be.
    Waiting {
        code: &'a GithubUserCode,
        address: &'a str,
        until: Option<String>,
    },
    /// GitHub approved it, as the account called `name`, once its record is here, which is the default if `default`.
    Approved {
        name: Option<&'a str>,
        default: bool,
    },
    Refused,
    /// Its code ended first, or the hub stopped while it waited.
    Ended,
    Cancelled,
    /// The hub couldn't go on, as this says.
    Failed(&'a str),
    /// The hub no longer has its record, as once another sign-in has started since it ended.
    Gone,
    /// In a state from a newer hub.
    Unknown,
}

impl<'a> Stands<'a> {
    /// How the sign-in `panel` shows stands, by the hub's `records` and its settings for GitHub, with times in the local
    /// time zone.
    pub(super) fn of(
        panel: &'a SignInPanel,
        records: &'a Snapshot,
        github: Option<&GithubSettings>,
        cx: &App,
    ) -> Stands<'a> {
        if let Some(why) = &panel.not_started {
            return Stands::NotStarted(why);
        }
        let Some(sign_in) = records.github_sign_in(&panel.id) else {
            return if panel.starting { Stands::Starting } else { Stands::Gone };
        };
        match &sign_in.status {
            GithubSignInStatus::Waiting { code, address, code_ends_at_ms } => {
                Stands::Waiting { code, address, until: look::time_of_day(*code_ends_at_ms, cx) }
            }
            GithubSignInStatus::Approved { account } => Stands::Approved {
                name: records.github_account(account).map(|account| account.user.name.as_str()),
                default: github.is_some_and(|github| github.account.as_ref() == Some(account)),
            },
            GithubSignInStatus::Refused => Stands::Refused,
            GithubSignInStatus::Ended => Stands::Ended,
            GithubSignInStatus::Cancelled => Stands::Cancelled,
            GithubSignInStatus::Failed { message } => Stands::Failed(message),
            GithubSignInStatus::Unknown => Stands::Unknown,
        }
    }

    /// Whether the hub may still wait for the person, so the panel offers Cancel.
    pub(super) fn may_wait(&self) -> bool {
        matches!(self, Stands::Starting | Stands::Waiting { .. })
    }

    /// Whether it ended in a way signing in again might change, so the panel offers that.
    pub(super) fn may_try_again(&self) -> bool {
        matches!(self, Stands::NotStarted(_) | Stands::Refused | Stands::Ended | Stands::Failed(_))
    }

    /// The panel's title.
    pub(super) fn title(&self) -> &'static str {
        if matches!(self, Stands::Approved { .. }) { "Signed in with GitHub" } else { "Sign in with GitHub" }
    }

    /// What the panel says under its title. `again` is the username of the account it's to sign in as again, if it
    /// was started from that account's row.
    pub(super) fn lead(&self, again: Option<&str>) -> String {
        match self {
            Stands::Starting => "Asking GitHub for a code…".to_string(),
            Stands::NotStarted(why) | Stands::Failed(why) => why.to_string(),
            Stands::Waiting { .. } => match again {
                Some(name) => format!(
                    "Enter this code on GitHub, signed in as {name}, in a browser on any device. Roundhouse never \
                     sees your password."
                ),
                None => "Enter this code on GitHub, in a browser on any device. Roundhouse never sees your password."
                    .to_string(),
            },
            Stands::Approved { name: Some(name), default: true } => {
                format!("Signed in as {name}. It's the default, so agents push as it on every machine.")
            }
            Stands::Approved { name: Some(name), default: false } => format!("Signed in as {name}."),
            Stands::Approved { name: None, .. } => "Signed in.".to_string(),
            Stands::Refused => "You refused it on GitHub, so nothing was connected.".to_string(),
            Stands::Ended => {
                "Its code ended before it was approved on GitHub, so nothing was connected. Sign in again for a new \
                 code."
                    .to_string()
            }
            Stands::Cancelled => "It was cancelled, so nothing was connected.".to_string(),
            Stands::Gone => "The hub no longer has this sign-in, as once another has started since.".to_string(),
            Stands::Unknown => {
                "The hub says something of this sign-in that this version of Roundhouse can't read.".to_string()
            }
        }
    }
}

/// What the line under a waiting sign-in's code says, until when it works, if that can be said.
pub(super) fn waiting(until: Option<&str>) -> String {
    match until {
        Some(until) => format!("Waiting for you to approve it on GitHub. The code works until {until}."),
        None => "Waiting for you to approve it on GitHub.".to_string(),
    }
}

/// What a panel says when the hub couldn't start a sign-in: what a conflict and an older hub mean for one, and
/// otherwise why, as for anything asked.
pub(super) fn not_started(e: &ClientError) -> String {
    match e {
        ClientError::Api(api) if api.code == ErrorCode::Conflict => {
            "As many sign-ins as the hub allows are waiting. Finish or cancel one, then try again.".to_string()
        }
        e if e.is_unsupported() => "This hub can't sign in with GitHub until it's updated. Use a token.".to_string(),
        e => refusals::couldnt("start signing in", e),
    }
}
