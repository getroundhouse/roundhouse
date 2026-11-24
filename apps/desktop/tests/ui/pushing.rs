//! What a workspace says when agents can't push to its repository on GitHub as the default GitHub account: nothing on
//! the local provider, against a real hub that checks with a stand-in for GitHub, and a line on a provider where
//! Roundhouse sets up GitHub, against a fake hub that says what it found. Every token is plainly made up.

#![cfg(test)]

use client_core::fake_hub::World;
use desktop::workspace_pane::{INSTALL_APP, OPEN_GIT};
use gpui_kit::TestAppContext;
use hub::{GithubAnswer, GithubScript};
use protocol::fixtures::observed;
use protocol::model::{GithubRepositoryName, GithubToken, InstalledApp, Reach, SessionStatus};

use crate::support::{Stack, Ui, git, open_on_the_world};

const TOKEN: &str = "github_pat_made_up_for_a_test";

/// The hub checks the repository with the default, and GitHub refuses `ada`, but a workspace on the local provider
/// says nothing of it, since Roundhouse leaves GitHub on this machine to the machine, as the hub's settings say. The
/// repository's `origin` is `cli/cli` on GitHub. Nothing has been fetched from it, so the workspace starts from the
/// checkout's own branch, and nothing reaches GitHub.
#[gpui_kit::test]
fn a_workspace_on_this_machine_says_nothing_of_whether_the_default_can_push(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let refused = GithubAnswer::git_says(403, "Permission to cli/cli.git denied to ada.");
    let script = GithubScript {
        users: vec![(TOKEN, GithubAnswer::user(7, "ada", None))],
        pushes: vec![("cli/cli", vec![refused])],
        ..GithubScript::default()
    };
    let _github = stack.start_hub_with_stand_in_github(script);
    let repo = stack.repo();
    git(&repo, &["remote", "add", "origin", "https://github.com/cli/cli.git"]);
    let client = stack.client();
    client.github_account_connect(GithubToken::new(TOKEN), None).unwrap();
    let id = client.repo_add(repo.to_str().unwrap()).unwrap().id;
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);

    ui.new_workspace();
    ui.wait("the refusal checked", |ui| {
        matches!(ui.view().records().github_reach(&id), Some(Reach::Refused { .. })) && ui.pane().is_some()
    });
    ui.redraw();

    assert!(!ui.has(OPEN_GIT), "no line on this machine");
}

/// A token's refusal, on a provider where Roundhouse sets up GitHub, which a fake hub's workspace is on, since its
/// settings name no provider left to its machine: the line, and from its button to where it says what to do.
#[gpui_kit::test]
fn a_workspace_says_why_agents_cant_push_and_its_line_opens_git(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("cli");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let repository = GithubRepositoryName::named("cli/cli");
    let reason = "Permission to cli/cli.git denied to ada.".to_string();
    let reach = Reach::Refused { repository, username: "ada".into(), reason, installation: None };
    world.observe(observed::reach(repo, reach));
    let (_fake, mut ui) = open_on_the_world(cx, world);

    ui.wait("the line saying so", |ui| ui.has(OPEN_GIT));
    let installs = ui.has(INSTALL_APP);
    ui.click(OPEN_GIT);

    ui.wait("Git in Settings", |ui| ui.has("git-connect"));
    assert!(ui.has("settings-back"), "in Settings, with the way back");
    assert!(!installs, "a token's line has no way to an app's page");
}

/// An app of your own's installation on adalabs, the default, refused `adalabs/app`, which the person didn't give it
/// there, so the line has a way to GitHub's page for installing the app, where they do that. Which installation the
/// hub finds, and what it says of it, is the hub's own tests', `github_reach`'s among them, so this opens on a fake hub
/// that says it.
#[gpui_kit::test]
fn an_installations_line_opens_where_its_app_is_installed(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("app");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let bot = "ada-roundhouse[bot]";
    let installation = Some(InstalledApp { slug: "ada-roundhouse".into(), on: "adalabs".into() });
    let reason = format!("Permission to adalabs/app.git denied to {bot}.");
    let repository = GithubRepositoryName::named("adalabs/app");
    let reach = Reach::Refused { repository, username: bot.into(), reason, installation };
    world.observe(observed::reach(repo, reach));
    let (_fake, mut ui) = open_on_the_world(cx, world);

    ui.wait("the line saying so", |ui| ui.has(INSTALL_APP));
    ui.click(INSTALL_APP);

    assert_eq!(ui.cx.opened_url().as_deref(), Some("https://github.com/apps/ada-roundhouse/installations/new"));
    assert!(ui.has(OPEN_GIT), "and Git in Settings beside it");
}
