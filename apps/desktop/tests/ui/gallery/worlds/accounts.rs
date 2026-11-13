//! The accounts agents run under, as Accounts in Settings lists them: Claude Code's own login, found as an agent
//! launched under it, and the logins the person gave the hub, in each state one can be in. Their tokens are the hub's,
//! so none is here.

use protocol::fixtures::account;
use protocol::model::{Account, AgentKind, HeldLogin, LimitWindow, UsageLimit};

use super::{context, conversation};
use crate::gallery::world::{HOUR_MS, NOW_MS, World};

const DAY_MS: i64 = 24 * HOUR_MS;
/// How long a token `claude setup-token` makes lasts, as the hub takes it to.
const YEAR_MS: i64 = 365 * DAY_MS;

/// The conversation, with Claude Code's own login on the machine, and four logins the person gave the hub: one it
/// holds, one about to end, one that has ended, and one disconnected.
pub(in crate::gallery) fn accounts() -> World {
    let mut world = conversation();
    world.accounts = vec![found(), work(), personal(), side_project(), contractor()];
    world
}

/// The conversation, with one login the person gave the hub, and none found where agents ran, as on a server with no
/// login of its own.
pub(in crate::gallery) fn none_found() -> World {
    let mut world = conversation();
    world.accounts = vec![work()];
    world
}

/// The conversation, on a hub from before hubs kept logins, which describes Claude with nothing that makes one.
pub(in crate::gallery) fn logins_unkept() -> World {
    let mut world = conversation();
    world.keeps_no_logins = true;
    world
}

/// The context's world, with the chat under a login the person gave the hub, which says only the name they gave it,
/// before any agent under it has heard of a limit.
pub(in crate::gallery) fn context_held_login() -> World {
    let mut world = context();
    let account = Account { limits: Vec::new(), ..work() };
    world.sessions.last_mut().unwrap().account = Some(account.id.clone());
    world.accounts = vec![account];
    world
}

/// Claude Code's own login on the machine, which the hub read as an agent launched under it.
pub(in crate::gallery) fn found() -> Account {
    Account {
        model_provider: Some("Anthropic API".to_string()),
        plan: Some("Max".to_string()),
        login: Some("Claude account".to_string()),
        email: Some("ada@example.com".to_string()),
        limits: vec![limit(LimitWindow::FiveHours, 29, 3 * HOUR_MS), limit(LimitWindow::SevenDays, 4, 150 * HOUR_MS)],
        used_at_ms: Some(NOW_MS - 20 * HOUR_MS),
        ..account("claude-0000000000000000")
    }
}

/// A login the hub holds, connected three weeks ago, in use a few minutes ago.
fn work() -> Account {
    Account {
        limits: vec![limit(LimitWindow::FiveHours, 62, 2 * HOUR_MS), limit(LimitWindow::SevenDays, 18, 96 * HOUR_MS)],
        held_login: Some(held(NOW_MS - 21 * DAY_MS)),
        used_at_ms: Some(NOW_MS - 9 * 60_000),
        ..added("claude-login-0001", "Work")
    }
}

/// A login the hub holds that ends in nine days, which no agent under it has used yet.
fn personal() -> Account {
    Account { held_login: Some(held(NOW_MS + 9 * DAY_MS - YEAR_MS)), ..added("claude-login-0002", "Personal") }
}

/// A login the hub holds that ended three days ago.
fn side_project() -> Account {
    Account {
        limits: vec![limit(LimitWindow::FiveHours, 0, 2 * HOUR_MS), limit(LimitWindow::SevenDays, 36, 100 * HOUR_MS)],
        held_login: Some(held(NOW_MS - 3 * DAY_MS - YEAR_MS)),
        used_at_ms: Some(NOW_MS - 5 * DAY_MS),
        ..added("claude-login-0003", "Side project")
    }
}

/// A login the hub was told to forget, whose limits started again since an agent under it last heard them.
fn contractor() -> Account {
    Account {
        limits: vec![limit(LimitWindow::SevenDays, 71, -2 * DAY_MS)],
        used_at_ms: Some(NOW_MS - 12 * DAY_MS),
        ..added("claude-login-0004", "Contractor")
    }
}

/// A login the hub was given at `connected_at_ms`, which ends a year on.
fn held(connected_at_ms: i64) -> HeldLogin {
    HeldLogin { connected_at_ms, ends_at_ms: Some(connected_at_ms + YEAR_MS) }
}

/// An account whose login the person gave the hub, called `name`, which a token says nothing more of.
fn added(id: &str, name: &str) -> Account {
    Account { name: Some(name.to_string()), login_agent: Some(AgentKind::Claude), ..account(id) }
}

/// A limit `used_percent` used, which starts again `resets_in_ms` from now, as an account's record has it.
fn limit(window: LimitWindow, used_percent: u8, resets_in_ms: i64) -> UsageLimit {
    UsageLimit { window, model: None, used_percent, resets_at_ms: Some(NOW_MS + resets_in_ms), heard_at_ms: None }
}
