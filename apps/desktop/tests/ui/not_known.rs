//! What the window draws of what a hub of version 5 sends as observations, which can be not known where a record's
//! field couldn't say so, and that it draws nothing a record holds in place of one, on a fake hub, which fills each such
//! field with `OLD_FORM`, as a hub that speaks version 4 too fills them for clients of that version.

#![cfg(test)]

use client_core::fake_hub::{OLD_FORM, World};
use gpui_kit::TestAppContext;
use protocol::capability::settings::ModelCatalog;
use protocol::fixtures::{account, github_account};
use protocol::model::{
    AgentKind, ContextUsage, GithubSettings, GithubUser, GithubUserId, Observation, Observed, Rev, SentCredential,
    SessionStatus,
};

use crate::settings::open_settings;
use crate::support::{FakeHub, Ui, one_chat, open_on_the_chat};

/// A chat, under an account the hub knows who it is of, in a workspace whose worker is another build, with a GitHub
/// account that's the default and the update feed offering a release, each said as an observation.
fn every_kind_said() -> (World, protocol::model::SessionId) {
    let (mut world, chat) = one_chat(SessionStatus::Idle, Vec::new());
    let mut claude = account("claude-1");
    (claude.plan, claude.email) = (Some("Max".into()), Some("ada@example.com".into()));
    world.accounts = vec![claude];
    let ada = github_account("github-0001");
    let sent = SentCredential::Account { account: ada.id.clone() };
    world.github = Some(GithubSettings {
        account: Some(ada.id.clone()),
        sent: Some(sent),
        left_to_machine: Vec::new(),
        set_up: Vec::new(),
    });
    world.github_accounts = vec![ada];
    world.workspaces[0].worker_build = Some("0.9.0".into());
    world.sessions[0].context = Some(ContextUsage {
        used_tokens: 16_000,
        max_tokens: 200_000,
        model_tokens: None,
        max_set_by: None,
        parts: Vec::new(),
    });
    (world, chat)
}

/// What the window shows, which has nothing a record holds in place of an observation.
fn shows_no_old_form(ui: &mut Ui<'_>, at: &str) {
    let shown = ui.texts_shown();
    assert!(!shown.contains(OLD_FORM), "{at} shows what a record holds in place of an observation: {shown}");
}

/// The chat, the workspace's card, and Agents and Git in Settings each draw what the observations say, and nothing of
/// the fields the hub filled.
#[gpui_kit::test]
fn no_screen_shows_what_a_record_holds_in_place_of_an_observation(cx: &mut TestAppContext) {
    let (world, chat) = every_kind_said();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);

    shows_no_old_form(&mut ui, "the chat");
    ui.hover("workspace-title");
    ui.wait_out_a_tooltip();
    ui.wait("the card", |ui| ui.has("workspace-card"));
    shows_no_old_form(&mut ui, "the workspace's card");
    open_settings(&mut ui);
    ui.click("settings-page-agents");
    shows_no_old_form(&mut ui, "Agents");
    ui.click("settings-page-git");
    ui.wait("the GitHub account", |ui| ui.has("git-account-github-0001"));
    shows_no_old_form(&mut ui, "Git");
}

/// A GitHub account whose username isn't known is drawn by its user's number, after those named.
#[gpui_kit::test]
fn a_github_account_whose_username_isnt_known_is_drawn_by_its_number(cx: &mut TestAppContext) {
    let (mut world, _) = one_chat(SessionStatus::Idle, Vec::new());
    let named = github_account("github-0001");
    let mut unnamed = github_account("github-0002");
    unnamed.user = GithubUser { id: GithubUserId::new(7), name: "grace".into() };
    let username = Observed::NotKnown;
    world.observations.push(Observation::GithubUsername { account: unnamed.id.clone(), username, rev: Rev::new(1) });
    world.github = Some(GithubSettings {
        account: Some(named.id.clone()),
        sent: None,
        left_to_machine: Vec::new(),
        set_up: Vec::new(),
    });
    world.github_accounts = vec![unnamed, named];
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);

    open_settings(&mut ui);
    ui.click("settings-page-git");
    ui.wait("both accounts", |ui| ui.has("git-account-github-0002") && ui.has("git-account-github-0001"));

    let shown = ui.texts_shown();
    assert!(shown.contains("#7") && !shown.contains("grace"), "{shown}");
    let view = ui.view();
    let records = view.records();
    let order: Vec<String> = records.github_accounts().iter().map(|a| a.id.to_string()).collect();
    assert_eq!(order, ["github-0001", "github-0002"], "the unnamed one after the named");
}

/// The model picker says `Loading models…` while a lookup has answered and its list hasn't come.
#[gpui_kit::test]
fn the_model_picker_waits_for_a_list_a_lookup_answered(cx: &mut TestAppContext) {
    let (mut world, _) = one_chat(SessionStatus::Idle, Vec::new());
    world.agents = Some(agents::descriptions());
    world.offer_models(AgentKind::Codex, &ModelCatalog::default());
    for list in &mut world.model_lists {
        list.catalog = None;
    }
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);

    ui.open_new_workspace();
    ui.pick_item("new-workspace-agent", "Codex");

    ui.wait("Loading models…", |ui| ui.has("load-models"));
    assert!(ui.texts_shown().contains("Loading models…"));
    assert!(!ui.has("new-workspace-model"), "nothing to pick from");
}
