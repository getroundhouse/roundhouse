//! What the window draws of what a hub sends as observations where one isn't known, which a record's field couldn't
//! say, on a fake hub.

#![cfg(test)]

use gpui_kit::TestAppContext;
use protocol::capability::settings::ModelCatalog;
use protocol::fixtures::{github_account, github_user, observed};
use protocol::model::{AgentKind, GithubSettings, Observation, Observed, Rev, SessionStatus};

use crate::settings::open_settings;
use crate::support::{FakeHub, Ui, one_chat};

/// A GitHub account whose username isn't known is drawn by its user's number, after those named.
#[gpui_kit::test]
fn a_github_account_whose_username_isnt_known_is_drawn_by_its_number(cx: &mut TestAppContext) {
    let (mut world, _) = one_chat(SessionStatus::Idle, Vec::new());
    let named = github_account("github-0001");
    world.observe(observed::username(named.id.clone(), "ada"));
    let mut unnamed = github_account("github-0002");
    unnamed.user = github_user(7);
    let username = Observed::NotKnown;
    world.observe(Observation::GithubUsername { account: unnamed.id.clone(), username, rev: Rev::new(1) });
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
    assert!(shown.contains("#7"), "{shown}");
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
    world.observations.retain(|observation| !matches!(observation, Observation::ModelCatalog { .. }));
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);

    ui.open_new_workspace();
    ui.pick_item("new-workspace-agent", "Codex");

    ui.wait("Loading models…", |ui| ui.has("load-models"));
    assert!(ui.texts_shown().contains("Loading models…"));
    assert!(!ui.has("new-workspace-model"), "nothing to pick from");
}
