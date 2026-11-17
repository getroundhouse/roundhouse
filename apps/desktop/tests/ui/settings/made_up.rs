//! What a fake hub holds for a test of what Settings draws, or of where its keys go, which asks the hub to change
//! nothing it records: a workspace whose agent thought before it answered, and the logins of Agents and the GitHub
//! accounts of Git. Every account is plainly made up, and none has a token here, since a hub keeps those.

#![cfg(test)]

use client_core::fake_hub::events::{finished_turn, taken, text, thought, user_prompt};
use client_core::fake_hub::{NOW_MS, World};
use gpui_kit::TestAppContext;
use protocol::fixtures::{account, github_account};
use protocol::model::{
    Account, AccountDefault, AgentKind, GithubAccount, GithubSettings, HeldLogin, SentCredential, Seq, SessionStatus,
};

use super::agents::to_agents;
use crate::support::{FakeHub, Ui};

pub(super) const DAY_MS: i64 = 24 * 60 * 60_000;

/// A workspace with one chat, whose agent thought, then answered.
pub(super) fn thought_then_answered() -> World {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let events = vec![
        user_prompt("Ponder it"),
        taken(Seq::new(1)),
        thought("msg-1", "Weighing the levers"),
        text("msg-2", "first answer"),
        finished_turn(4, 0.01),
    ];
    world.chat(&workspace, SessionStatus::Idle, events);
    world
}

/// A window on [`thought_then_answered`], once its chat shows. The hub goes with the test, so it's returned to be kept.
pub(super) fn in_a_workspace(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>) {
    let fake = FakeHub::holding(thought_then_answered());
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the answer", |ui| ui.assistant_text().contains("first answer"));
    (fake, ui)
}

/// A login the person gave the hub, called `name`, which it holds, and which ends `ends_in_ms` from now, a year after
/// it was connected.
pub(super) fn added(id: &str, name: &str, ends_in_ms: i64) -> Account {
    let ends_at_ms = NOW_MS + ends_in_ms;
    Account {
        name: Some(name.to_string()),
        login_agent: Some(AgentKind::Claude),
        held_login: Some(HeldLogin { connected_at_ms: ends_at_ms - 365 * DAY_MS, ends_at_ms: Some(ends_at_ms) }),
        ..account(id)
    }
}

/// A hub holding `accounts`, the first Claude's default.
pub(super) fn holding_logins(accounts: Vec<Account>) -> World {
    let mut world = World::default();
    world.account_defaults = vec![AccountDefault { agent: AgentKind::Claude, account: accounts[0].id.clone() }];
    world.accounts = accounts;
    world
}

/// The GitHub account `ada`, whose token the hub holds, as the default, on a hub that sends workers its login.
pub(super) fn with_ada(world: &mut World) -> GithubAccount {
    let ada = github_account("github-0001");
    let sent = SentCredential::Account { account: ada.id.clone() };
    world.github = Some(GithubSettings { account: Some(ada.id.clone()), sent: Some(sent) });
    world.github_accounts = vec![ada.clone()];
    ada
}

/// A window on `world`, in Agents, once the hub has said what makes a login, so Connect account is on.
pub(super) fn in_agents_of(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    to_agents(&mut ui);
    (fake, ui)
}
