//! The accounts agents run under, as Agents in Settings lists them: Claude Code's own login, found as an agent
//! launched under it, and the logins the person gave the hub, in each state one can be in. Their tokens are the hub's,
//! so none is here.

use protocol::fixtures::{account, claude_default, observed};
use protocol::model::{
    Account, AccountDefault, AccountIdentity, AccountSource, AgentKind, HeldLogin, LimitWindow, Observation, Reserve,
    Seq, SessionStatus, UsageLimit,
};

use super::{context, conversation};
use client_core::fake_hub::events::user_prompt;
use client_core::fake_hub::{HOUR_MS, NOW_MS, World};
use protocol::event::{EventBody, ExitReason};

/// What the hub says of a launch that Automatic found no account with usage to spare for.
const NO_ACCOUNT_TO_SPARE: &str =
    "no account has usage to spare: each is down to what it reserves of its week, or has used all of a limit";

const DAY_MS: i64 = 24 * HOUR_MS;
/// How long a token `claude setup-token` makes lasts, as the hub takes it to.
const YEAR_MS: i64 = 365 * DAY_MS;

/// An account's record, and what agents under it said of it: who it is, and what's left of its limits.
#[derive(Clone)]
pub(in crate::gallery) struct Heard {
    pub(in crate::gallery) account: Account,
    pub(in crate::gallery) identity: Option<AccountIdentity>,
    pub(in crate::gallery) limits: Option<Vec<UsageLimit>>,
}

impl Heard {
    /// `account`, which no agent under it has said anything of.
    fn of(account: Account) -> Self {
        Self { account, identity: None, limits: None }
    }

    /// The same, with `limits` heard.
    fn limited(self, limits: Vec<UsageLimit>) -> Self {
        Self { limits: Some(limits), ..self }
    }

    /// The same, with its record changed by `change`.
    fn with(self, change: impl FnOnce(Account) -> Account) -> Self {
        Self { account: change(self.account), ..self }
    }

    /// What its hub observes of it.
    fn observations(&self) -> Vec<Observation> {
        let id = &self.account.id;
        let identity = self.identity.clone().map(|identity| observed::identity(id.clone(), identity));
        let limits = self.limits.clone().map(|limits| observed::limits(id.clone(), limits));
        identity.into_iter().chain(limits).collect()
    }
}

/// Has the world's hub hold `accounts`, in place of any it held, with what agents under each said of it.
pub(in crate::gallery) fn hold(world: &mut World, accounts: Vec<Heard>) {
    let of_an_account = |observation: &Observation| {
        matches!(observation, Observation::AccountIdentity { .. } | Observation::AccountLimits { .. })
    };
    world.observations.retain(|observation| !of_an_account(observation));
    world.observations.extend(accounts.iter().flat_map(Heard::observations));
    world.accounts = accounts.into_iter().map(|heard| heard.account).collect();
}

/// The conversation, with Claude Code's own login on the machine, and four logins the person gave the hub: one it
/// holds, one about to end, one that has ended, and one disconnected.
pub(in crate::gallery) fn accounts() -> World {
    let mut world = conversation();
    hold(&mut world, vec![found(), work(), personal(), side_project(), contractor()]);
    world.account_defaults = vec![claude_default(&work().account.id)];
    world
}

/// The conversation, with one login the person gave the hub, and none found where agents ran, as on a server with no
/// login of its own.
pub(in crate::gallery) fn none_found() -> World {
    let mut world = conversation();
    hold(&mut world, vec![work()]);
    world.account_defaults = vec![claude_default(&work().account.id)];
    world
}

/// The context's world, with the chat on a login the person gave the hub, the only one, which says only the name they
/// gave it, before any agent under it has heard of a limit.
pub(in crate::gallery) fn context_held_login() -> World {
    let mut world = context();
    let account = Heard { limits: None, ..work() };
    let chat = world.sessions.last_mut().unwrap();
    (chat.account, chat.account_source) = (Some(account.account.id.clone()), Some(AccountSource::Hub));
    hold(&mut world, vec![account]);
    world
}

/// The context's world, with the chat on Work, the default, and the person's other logins, so its limits' card offers
/// to move it to Personal. The one that has ended and the one disconnected can't be moved to, so the menu leaves them
/// out.
pub(in crate::gallery) fn context_to_move() -> World {
    let mut world = context();
    let chat = world.sessions.last_mut().unwrap();
    (chat.account, chat.account_source) = (Some(work().account.id), Some(AccountSource::Hub));
    hold(&mut world, vec![work(), personal(), side_project(), contractor()]);
    world.account_defaults = vec![claude_default(&work().account.id)];
    world
}

/// Claude Code's own login on the machine, which the hub read as an agent launched under it.
pub(in crate::gallery) fn found() -> Heard {
    let identity = AccountIdentity {
        model_provider: Some("Anthropic API".to_string()),
        plan: Some("Max".to_string()),
        login: Some("Claude account".to_string()),
        email: Some("ada@example.com".to_string()),
    };
    let account = Account { used_at_ms: Some(NOW_MS - 20 * HOUR_MS), ..account("claude-0000000000000000") };
    let limits = vec![limit(LimitWindow::FiveHours, 29, 3 * HOUR_MS), limit(LimitWindow::SevenDays, 4, 150 * HOUR_MS)];
    Heard { identity: Some(identity), ..Heard::of(account) }.limited(limits)
}

/// A login the hub holds, connected three weeks ago, in use a few minutes ago.
fn work() -> Heard {
    let account = Account {
        held_login: Some(held(NOW_MS - 21 * DAY_MS)),
        used_at_ms: Some(NOW_MS - 9 * 60_000),
        ..added("claude-login-0001", "Work")
    };
    let limits = vec![limit(LimitWindow::FiveHours, 62, 2 * HOUR_MS), limit(LimitWindow::SevenDays, 18, 96 * HOUR_MS)];
    Heard::of(account).limited(limits)
}

/// A login the hub holds that ends in nine days, which no agent under it has used yet.
fn personal() -> Heard {
    Heard::of(Account {
        held_login: Some(held(NOW_MS + 9 * DAY_MS - YEAR_MS)),
        ..added("claude-login-0002", "Personal")
    })
}

/// A login the hub holds that ended three days ago.
fn side_project() -> Heard {
    let account = Account {
        held_login: Some(held(NOW_MS - 3 * DAY_MS - YEAR_MS)),
        used_at_ms: Some(NOW_MS - 5 * DAY_MS),
        ..added("claude-login-0003", "Side project")
    };
    let limits = vec![limit(LimitWindow::FiveHours, 0, 2 * HOUR_MS), limit(LimitWindow::SevenDays, 36, 100 * HOUR_MS)];
    Heard::of(account).limited(limits)
}

/// A login the hub was told to forget, whose limits started again since an agent under it last heard them.
fn contractor() -> Heard {
    let account = Account { used_at_ms: Some(NOW_MS - 12 * DAY_MS), ..added("claude-login-0004", "Contractor") };
    Heard::of(account).limited(vec![limit(LimitWindow::SevenDays, 71, -2 * DAY_MS)])
}

/// A login the hub was given at `connected_at_ms`, which ends a year on.
fn held(connected_at_ms: i64) -> HeldLogin {
    HeldLogin { connected_at_ms, ends_at_ms: Some(connected_at_ms + YEAR_MS) }
}

/// An account whose login the person gave the hub, called `name`, which a token says nothing more of.
fn added(id: &str, name: &str) -> Account {
    Account { name: Some(name.to_string()), login_agent: Some(AgentKind::Claude), ..account(id) }
}

/// A limit `used_percent` used, which starts again `resets_in_ms` from now, as an agent heard it.
fn limit(window: LimitWindow, used_percent: u8, resets_in_ms: i64) -> UsageLimit {
    UsageLimit { window, model: None, used_percent, resets_at_ms: Some(NOW_MS + resets_in_ms), heard_at_ms: None }
}

/// Claude Code's own login, and the logins the person gave the hub, with Work reserving a quarter of its week, which
/// has most of it left, and Personal half, which it's down to.
pub(in crate::gallery) fn reserves() -> World {
    let mut world = conversation();
    hold(&mut world, vec![found(), work_reserving(), personal_reserving(), side_project(), contractor()]);
    world.account_defaults = vec![claude_default(&work().account.id)];
    world
}

/// The context's world, with the chat on Personal, which is down to the half of its week it reserves, and Work, which
/// the chat could move to.
pub(in crate::gallery) fn context_in_reserve() -> World {
    let mut world = context();
    let chat = world.sessions.last_mut().unwrap();
    (chat.account, chat.account_source) = (Some(personal().account.id), Some(AccountSource::Hub));
    hold(&mut world, vec![work_reserving(), personal_reserving()]);
    world.account_defaults = vec![claude_default(&work().account.id)];
    world
}

/// The reserves' world, with Claude's chats put on accounts automatically.
pub(in crate::gallery) fn automatic() -> World {
    let mut world = reserves();
    world.account_defaults = vec![automatically()];
    world
}

/// The context's world, with the chat on Work, which Automatic chose, and Personal, which the chat could move to.
pub(in crate::gallery) fn context_placed_automatically() -> World {
    let mut world = context();
    let chat = world.sessions.last_mut().unwrap();
    (chat.account, chat.account_source, chat.placed_automatically) =
        (Some(work().account.id), Some(AccountSource::Hub), true);
    hold(&mut world, vec![work_reserving(), personal_reserving()]);
    world.account_defaults = vec![automatically()];
    world
}

/// Claude's chats put on accounts automatically, with Work the account chosen before.
fn automatically() -> AccountDefault {
    AccountDefault { automatic: true, ..claude_default(&work().account.id) }
}

/// Work, reserving a quarter of its week.
fn work_reserving() -> Heard {
    work().with(|account| Account { reserve: Reserve::new(25), ..account })
}

/// Personal, a login the hub holds, reserving half its week, which it's down to.
fn personal_reserving() -> Heard {
    let account = Account {
        held_login: Some(held(NOW_MS - 40 * DAY_MS)),
        used_at_ms: Some(NOW_MS - 2 * HOUR_MS),
        reserve: Reserve::new(50),
        ..added("claude-login-0002", "Personal")
    };
    let limits = vec![limit(LimitWindow::FiveHours, 10, 3 * HOUR_MS), limit(LimitWindow::SevenDays, 59, 60 * HOUR_MS)];
    Heard::of(account).limited(limits)
}

/// A new chat that couldn't start, since Automatic found no account with usage to spare: Work and Personal are down to
/// what they reserve, and Side project is out of its five hours. What was sent was dropped.
pub(in crate::gallery) fn needs_an_account() -> World {
    let prompt = "Add a retry with backoff to the webhook sender, and a test that it gives up after five tries.";
    let dropped = EventBody::PromptDropped { prompt: Seq::FIRST };
    let failed = EventBody::AgentExited {
        reason: ExitReason::StartFailed,
        code: None,
        signal: None,
        stderr_tail: vec![NO_ACCOUNT_TO_SPARE.to_string()],
    };
    let mut world = super::chat_world(SessionStatus::Interrupted, vec![user_prompt(prompt), dropped, failed]);
    world.sessions.last_mut().unwrap().needs_an_account = true;
    let work = work_reserving()
        .limited(vec![limit(LimitWindow::FiveHours, 20, 2 * HOUR_MS), limit(LimitWindow::SevenDays, 78, 30 * HOUR_MS)]);
    hold(&mut world, vec![work, personal_reserving(), side_project_out()]);
    world.account_defaults = vec![automatically()];
    world
}

/// The same, with every account out of a limit.
pub(in crate::gallery) fn every_account_out() -> World {
    let mut world = needs_an_account();
    let work = work_reserving().limited(vec![
        limit(LimitWindow::FiveHours, 40, 2 * HOUR_MS),
        limit(LimitWindow::SevenDays, 100, 30 * HOUR_MS),
    ]);
    let personal = personal_reserving().limited(vec![
        limit(LimitWindow::FiveHours, 100, 3 * HOUR_MS),
        limit(LimitWindow::SevenDays, 70, 60 * HOUR_MS),
    ]);
    hold(&mut world, vec![work, personal, side_project_out()]);
    world
}

/// Side project, a login the hub holds, out of its five hours until two hours from now.
fn side_project_out() -> Heard {
    let account = Account {
        held_login: Some(held(NOW_MS - 60 * DAY_MS)),
        used_at_ms: Some(NOW_MS - HOUR_MS),
        ..added("claude-login-0003", "Side project")
    };
    let limits =
        vec![limit(LimitWindow::FiveHours, 100, 2 * HOUR_MS), limit(LimitWindow::SevenDays, 36, 100 * HOUR_MS)];
    Heard::of(account).limited(limits)
}
