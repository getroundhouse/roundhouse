//! Whose account an agent runs under, the login the hub holds for it, and how much of its limits is left.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{AccountId, Rev};

/// The account an agent's CLI is logged in to on its worker's machine, as the worker reads it. Every part is `None`
/// when the agent doesn't say, such as a plan for a login with an API key. Who it is, from `model_provider` on, is
/// what an [`Account`]'s record says of it too, so a part added here is added there.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccount {
    /// Which account it is. Agents under the same account share its limits, and an agent under another, even with
    /// the same model provider, has its own. `None` when nobody is logged in, or the agent can't tell one login from
    /// another.
    #[serde(default)]
    pub id: Option<AccountId>,
    /// Whether anyone is logged in.
    #[serde(default)]
    pub logged_in: bool,
    /// Who serves the model, for a person to read, such as `Anthropic API`. Not a provider in this project's sense,
    /// which is where a hub runs workers.
    #[serde(default)]
    pub model_provider: Option<String>,
    /// The subscription, for a person to read, such as `Max`.
    #[serde(default)]
    pub plan: Option<String>,
    /// How it's logged in, for a person to read, such as `Claude account`.
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

impl AgentAccount {
    /// Which account it is, when someone is logged in to one that can be told from another. Only such an account has
    /// a record, and limits kept for it.
    pub fn logged_in_as(&self) -> Option<&AccountId> {
        self.id.as_ref().filter(|_| self.logged_in)
    }
}

/// An account agents run under, as a record the hub keeps, so every client shows the same of it and it's there once
/// the agents have gone. Who it is comes from the worker, read each time an agent under it launches, and for a login
/// the hub holds, from the hub's record of that login too, since a token doesn't say whose it is. What's left of its
/// limits is what any agent under it last heard, in whichever workspace, since a limit is the account's rather than a
/// session's. Only a login that can be told from another has a record, so whoever it is is logged in. Who it is, from
/// `model_provider` on, is what [`AgentAccount`] says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    /// Who serves the model, for a person to read, as [`AgentAccount`] says.
    #[serde(default)]
    pub model_provider: Option<String>,
    /// The subscription, for a person to read.
    #[serde(default)]
    pub plan: Option<String>,
    /// How it's logged in, for a person to read.
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    /// What the person called the account as they connected a login to it, since a token doesn't say whose it is. It
    /// stays once the login is disconnected, since the sessions that ran under it still name the account.
    #[serde(default)]
    pub name: Option<String>,
    /// Shortest window first. Empty until an agent under the account has heard of one, and for an account with none.
    /// None says when it was heard, since hearing the same again isn't a change to the record.
    #[serde(default)]
    pub limits: Vec<UsageLimit>,
    /// The login the hub holds for the account and hands to workers, if it holds one. `None` for an account that
    /// agents run under through the login on their worker's machine, and for one whose login was disconnected.
    #[serde(default)]
    pub held_login: Option<HeldLogin>,
    #[serde(default)]
    pub rev: Rev,
}

/// What an [`Account`]'s record says of the login the hub holds for it. It never carries the token, which only the
/// hub and the workers it hands it to have.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldLogin {
    /// When the hub was given it, by the hub's clock.
    pub connected_at_ms: i64,
    /// When it stops working at the latest, by the hub's clock, if the hub knows. The CLI doesn't say when a token
    /// ends, so for one `claude setup-token` made, it's a year after it was connected, and sooner for one made before.
    #[serde(default)]
    pub ends_at_ms: Option<i64>,
}

/// A login the hub holds for an account, as it hands it to a worker for an agent or a one-off call to run under, so the
/// worker's machine needs no login of its own. The hub gives one only to a worker, which keeps it only while the agent
/// or the call it came with runs. No provider's adapter is given one, though one that carries a worker's stream itself
/// passes it on unread.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentLogin {
    /// A token the agent's own CLI made to last, such as `claude setup-token`'s, which the agent's adapter gives the CLI
    /// in its own way.
    pub token: LoginToken,
}

/// The token of a login the hub holds, which is all it takes to use the account, so it's kept out of logs: its `Debug`
/// says only that it's there, and a request or a reply that carries one can be logged whole. On the wire it's a
/// string.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct LoginToken(String);

impl LoginToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The token itself, for handing to the agent's CLI or keeping, and nothing else.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LoginToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LoginToken(..)")
    }
}

/// What a session's agent runs under: the account, and what the hub has heard of that account's limits, from any
/// agent under it. Kept for clients from before accounts were records, which ask for it. A client now reads
/// [`Session::account`](super::Session::account) and that account's record.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionAccount {
    pub account: AgentAccount,
    /// Shortest window first. Empty until an agent under the account has heard of one, and for an account with none.
    #[serde(default)]
    pub limits: Vec<UsageLimit>,
}

/// How much of one limit on an account's use is used up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageLimit {
    pub window: LimitWindow,
    /// From 0 to 100.
    pub used_percent: u8,
    /// When the window ends and the limit starts again.
    #[serde(default)]
    pub resets_at_ms: Option<i64>,
    /// When an agent heard this, by the hub's clock, which the hub adds as it keeps it. `None` from the agent, which
    /// says it as it hears it, and in an [`Account`]'s record.
    #[serde(default)]
    pub heard_at_ms: Option<i64>,
}

impl UsageLimit {
    /// How much is left, from 0 to 100.
    pub fn left_percent(&self) -> u8 {
        100_u8.saturating_sub(self.used_percent)
    }
}

/// How long a limit counts use over, shortest first.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LimitWindow {
    FiveHours,
    SevenDays,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whats_left_of_a_limit_is_the_rest_of_it() {
        let limit = |used_percent| UsageLimit {
            window: LimitWindow::SevenDays,
            used_percent,
            resets_at_ms: None,
            heard_at_ms: None,
        };

        assert_eq!(limit(4).left_percent(), 96);
        assert_eq!(limit(100).left_percent(), 0);
        assert_eq!(limit(140).left_percent(), 0);
    }

    #[test]
    fn an_account_that_says_nothing_still_reads_and_a_newer_window_is_unknown() {
        assert_eq!(serde_json::from_str::<AgentAccount>("{}").unwrap(), AgentAccount::default());
        assert_eq!(serde_json::from_str::<SessionAccount>(r#"{"account":{}}"#).unwrap(), SessionAccount::default());
        let newer: UsageLimit = serde_json::from_str(r#"{"window":"thirty_days","used_percent":3}"#).unwrap();

        assert_eq!(newer.window, LimitWindow::Unknown);
    }

    #[test]
    fn an_account_record_that_says_only_who_it_is_still_reads() {
        let account: Account = serde_json::from_str(r#"{"id":"claude-1"}"#).unwrap();

        assert_eq!(account.id, AccountId::from("claude-1"));
        assert_eq!((account.email, account.limits, account.rev), (None, Vec::new(), Rev::default()));
        assert_eq!((account.name, account.held_login), (None, None));
    }

    #[test]
    fn a_login_carries_its_token_on_the_wire_and_hides_it_from_debug() {
        let login = AgentLogin { token: LoginToken::new("a-made-up-token") };

        assert_eq!(serde_json::to_string(&login).unwrap(), r#"{"token":"a-made-up-token"}"#);
        assert_eq!(serde_json::from_str::<AgentLogin>(r#"{"token":"a-made-up-token"}"#).unwrap(), login);
        assert_eq!(login.token.as_str(), "a-made-up-token");
        assert!(!format!("{login:?}").contains("made-up"), "{login:?}");
    }

    #[test]
    fn windows_sort_shortest_first() {
        let mut windows = [LimitWindow::SevenDays, LimitWindow::FiveHours];
        windows.sort();

        assert_eq!(windows, [LimitWindow::FiveHours, LimitWindow::SevenDays]);
    }
}
