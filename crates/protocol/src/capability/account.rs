//! The account an agent runs under: who it is, how much of its limits is left, and the login the hub holds for it.

use serde::{Deserialize, Serialize};

use super::Claimed;
use super::area::area;
use crate::model::{LoginDescription, ModelLabel};

area! { "the account";
    /// Who the agent runs as, from a one-off call that starts no conversation: the account's ID, which is the same in
    /// every build, its email, its plan and its organization, as far as the agent says.
    Account => account: Claimed,
    /// How much of each of the account's limits is used, as a [`Limit`] each, when the agent hears, which is an event
    /// in the log, and when the adapter asks, as a [`Report::Limits`](super::Report::Limits), which the worker has it
    /// do while the agent works, at most every five minutes, where the agent doesn't say by itself. The hub keeps the
    /// newest on the account's record.
    Limits => limits: Claimed,
    /// What the account can spend past its limits, such as credits or extra usage, how much of it is used, and
    /// whether the agent goes on into it. The hub keeps the newest on the account's record.
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
