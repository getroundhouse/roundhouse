//! Whose account an agent runs under, and how much of its limits is left.

use serde::{Deserialize, Serialize};

use super::AccountId;

/// The account an agent's CLI is logged in to on its worker's machine. Every part is `None` when the agent doesn't
/// say, such as a plan for a login with an API key.
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

/// What a session's agent runs under: the account, and what the hub has heard of that account's limits, from any
/// agent under it.
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
    /// says it as it hears it.
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
    fn windows_sort_shortest_first() {
        let mut windows = [LimitWindow::SevenDays, LimitWindow::FiveHours];
        windows.sort();

        assert_eq!(windows, [LimitWindow::FiveHours, LimitWindow::SevenDays]);
    }
}
