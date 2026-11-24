//! What the page's unit tests are drawn from: a moment held still, logins, accounts and how a hub describes Claude,
//! each plainly made up.

use client_core::records::{HubRecords, Snapshot};
use protocol::fixtures::account;
use protocol::model::{
    Account, AgentDescription, AgentKind, HeldLogin, LimitWindow, LoginDescription, Observation, Records, UsageLimit,
};

/// Sunday 1 November 2026, at 3 PM in UTC, which held still is the local time zone.
pub(super) const NOW_MS: i64 = 1_793_545_200_000;
pub(super) const MINUTE_MS: i64 = 60_000;
pub(super) const DAY_MS: i64 = 24 * 60 * MINUTE_MS;

/// An account whose login the person gave the hub, called `name`, which the hub holds.
pub(super) fn added(id: &str, name: &str) -> Account {
    Account {
        name: Some(name.to_string()),
        login_agent: Some(AgentKind::Claude),
        held_login: Some(HeldLogin { connected_at_ms: 0, ends_at_ms: None }),
        ..account(id)
    }
}

/// A client's copy of a hub's records that holds `accounts`, which the page reads them from, and what `observed` says
/// of them, such as who each is.
pub(super) fn holding<'a>(
    accounts: impl IntoIterator<Item = &'a Account>,
    observed: impl IntoIterator<Item = Observation>,
) -> Snapshot {
    let mut held = HubRecords::default();
    let (accounts, observations) = (accounts.into_iter().cloned().collect(), observed.into_iter().collect());
    held.apply_list(Records { accounts, observations, ..Records::default() });
    held.snapshot()
}

/// A limit of `window` with `used_percent` of it used, which starts again at `resets_at_ms`.
pub(super) fn limit(window: LimitWindow, used_percent: u8, resets_at_ms: i64) -> UsageLimit {
    UsageLimit { window, model: None, used_percent, resets_at_ms: Some(resets_at_ms), heard_at_ms: None }
}

/// Claude as a hub describes it, with what makes a token for it, as `login` says, if anything.
pub(super) fn claude(login: Option<LoginDescription>) -> AgentDescription {
    AgentDescription { login, ..protocol::fixtures::claude() }
}
