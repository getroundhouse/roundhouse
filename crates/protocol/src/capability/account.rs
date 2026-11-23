//! The account an agent runs under: who it is, how much of its limits is left, and the login the hub holds for it.

use serde::{Deserialize, Serialize};

use super::Claimed;
use super::area::area;
use crate::model::{LimitWindow, LoginDescription, ModelLabel, UsageLimit};

area! { "the account";
    /// Who the agent runs as, from a one-off call that starts no conversation: the account's ID, which is the same in
    /// every build, its email, its plan and its organization, as far as the agent says.
    Account => account: Claimed,
    /// How much of each of the account's limits is used, as a [`Limit`] each, in a [`Report::Limits`](super::Report::Limits),
    /// when the agent hears, and when the adapter asks, which the worker has it do while the agent works, at most every
    /// five minutes, where the agent doesn't say by itself. The hub keeps the newest of each, by the account, as an
    /// observation.
    Limits => limits: Claimed,
    /// What the account can spend past its limits, such as credits or extra usage, how much of it is used, and
    /// whether the agent goes on into it. The hub keeps nothing of it, and drops the report.
    Credits => credits: Claimed,
    /// The hub can hold a login for the account, which the person makes as the [`LoginDescription`] says, and hand it
    /// to the agent for as long as it runs, so an agent on another machine needs no login of its own there.
    HeldLogin => held_login: LoginDescription,
    /// The agent runs models from several model providers, each signed in to apart, and each model says whose it is.
    ModelProviders => model_providers: Claimed,
}

/// How much of one of the account's limits is used. It takes the place of a fixed set of windows, since agents count
/// over windows of their own, such as Codex's in minutes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Limit {
    /// How long it counts use over, in minutes, such as 300 for five hours. `None` where the agent doesn't say.
    pub window_minutes: Option<u32>,
    /// The one model it counts, or `None` for every model the account runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelLabel>,
    /// From 0 to 100.
    pub used_percent: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at_ms: Option<i64>,
    /// What the agent calls it, such as `seven_day_opus`, for a person to read where nothing else names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Five hours, the window of the account's shortest limit.
const FIVE_HOURS: u32 = 5 * 60;
/// A week, the window of the account's limit on all its models, and of each model's own.
const A_WEEK: u32 = 7 * 24 * 60;

impl Limit {
    /// `limit`, as the account's record keeps it, by the window it counts over.
    pub fn of(limit: &UsageLimit) -> Self {
        let window_minutes = match limit.window {
            LimitWindow::FiveHours => Some(FIVE_HOURS),
            LimitWindow::SevenDays | LimitWindow::ModelSevenDays => Some(A_WEEK),
            LimitWindow::Unknown => None,
        };
        Limit {
            window_minutes,
            model: limit.model.clone(),
            used_percent: limit.used_percent,
            resets_at_ms: limit.resets_at_ms,
            name: None,
        }
    }

    /// As the account's record keeps it, whose windows are five hours, a week of every model, and a week of one model.
    /// Any other window is one the record has no name for.
    pub fn usage(&self) -> UsageLimit {
        let window = match (self.window_minutes, &self.model) {
            (Some(FIVE_HOURS), None) => LimitWindow::FiveHours,
            (Some(A_WEEK), None) => LimitWindow::SevenDays,
            (Some(A_WEEK), Some(_)) => LimitWindow::ModelSevenDays,
            _ => LimitWindow::Unknown,
        };
        let model = self.model.clone().filter(|_| window == LimitWindow::ModelSevenDays);
        UsageLimit {
            window,
            model,
            used_percent: self.used_percent,
            resets_at_ms: self.resets_at_ms,
            heard_at_ms: None,
        }
    }
}

/// What the account can spend past its limits.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Credits {
    pub state: CreditState,
    /// How much of a monthly allowance is used, from 0 to 100, where there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<u8>,
    /// What's left, as the agent words it, such as "$12.40", since accounts count in different units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreditState {
    /// The account has none, or doesn't use them.
    Off,
    /// The account has some, and the agent goes on into them once a limit is reached.
    On,
    /// The agent is spending them now, since a limit is reached.
    InUse,
    Unlimited,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A limit the account's record keeps reads back as it was, whatever its window, and one counted over a window the
    /// record has no name for is kept as one it doesn't know, rather than as another.
    #[test]
    fn a_limit_keeps_its_window_on_its_way_to_the_accounts_record() {
        let limit = |window, model: Option<&str>| UsageLimit {
            window,
            model: model.map(ModelLabel::new),
            used_percent: 40,
            resets_at_ms: Some(9),
            heard_at_ms: None,
        };
        for kept in [
            limit(LimitWindow::FiveHours, None),
            limit(LimitWindow::SevenDays, None),
            limit(LimitWindow::ModelSevenDays, Some("Fable")),
            limit(LimitWindow::Unknown, None),
        ] {
            assert_eq!(Limit::of(&kept).usage(), kept, "{:?}", kept.window);
        }

        let a_day = Limit { window_minutes: Some(24 * 60), ..Limit::of(&limit(LimitWindow::FiveHours, None)) };
        assert_eq!(a_day.usage().window, LimitWindow::Unknown);
    }
}
