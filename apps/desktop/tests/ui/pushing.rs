//! What a workspace says when agents can't push to its repository on GitHub as the default GitHub account, against a
//! real hub that checks with a stand-in for GitHub. Every token is plainly made up.

#![cfg(test)]

use desktop::workspace_pane::{INSTALL_APP, OPEN_GIT};
use gpui_kit::TestAppContext;
use hub::{GithubAnswer, GithubScript};
use protocol::client::github_apps::{GithubAppMake, GithubManifestCode};
use protocol::model::GithubToken;

use crate::support::{Stack, Ui, git};

const TOKEN: &str = "github_pat_made_up_for_a_test";

/// From the hub's check to the line in the workspace, and from its button to where it says what to do. The
/// repository's `origin` is `cli/cli` on GitHub, which refuses `ada`. Nothing has been fetched from it, so the
/// workspace starts from the checkout's own branch, and nothing reaches GitHub.
#[gpui_kit::test]
fn a_workspace_says_why_agents_cant_push_and_its_line_opens_git(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let refused = GithubAnswer::git_says(403, "Permission to cli/cli.git denied to ada.");
    let script = GithubScript {
        users: vec![(TOKEN, GithubAnswer::user(7, "ada", None))],
        pushes: vec![("cli/cli", vec![refused])],
        ..GithubScript::default()
    };
    let _github = stack.start_hub_with_stand_in_github(script);
    let repo = stack.repo();
    git(&repo, &["remote", "add", "origin", "git@github.com:cli/cli.git"]);
    let client = stack.client();
    client.github_account_connect(GithubToken::new(TOKEN), None).unwrap();
    client.repo_add(repo.to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);

    ui.new_workspace();
    ui.wait("the line saying so", |ui| ui.has(OPEN_GIT));
    let installs = ui.has(INSTALL_APP);
    ui.click(OPEN_GIT);

    ui.wait("Git in Settings", |ui| ui.has("git-connect"));
    assert!(ui.has("settings-back"), "in Settings, with the way back");
    assert!(!installs, "a token's line has no way to an app's page");
}

/// An app of your own's installation on adalabs, the default once the hub has made the app, refused `adalabs/app`,
/// which the person didn't give it there, so the line has a way to GitHub's page for installing the app, where they
/// do that. Nothing has been fetched from `origin`, as in the test above, so nothing reaches GitHub.
#[gpui_kit::test]
fn an_installations_line_opens_where_its_app_is_installed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let refused = GithubAnswer::git_says(403, "Permission to adalabs/app.git denied to ada-roundhouse[bot].");
    let script = GithubScript {
        conversions: vec![("a-code", GithubAnswer::app_made(9, "ada-roundhouse", (2, "adalabs")))],
        named: vec![("ada-roundhouse[bot]", GithubAnswer::user(41, "ada-roundhouse[bot]", None))],
        installations: vec![GithubAnswer::installations(&[(70, 2, "adalabs")])],
        minted: vec![(70, vec![GithubAnswer::installation_token("ghs_made_up", "2099-01-01T00:00:00Z")])],
        pushes: vec![("adalabs/app", vec![refused])],
        ..GithubScript::default()
    };
    let _github = stack.start_hub_with_stand_in_github(script);
    let repo = stack.repo();
    git(&repo, &["remote", "add", "origin", "git@github.com:adalabs/app.git"]);
    let client = stack.client();
    client.call(GithubAppMake { code: GithubManifestCode::new("a-code") }).unwrap();
    client.repo_add(repo.to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);

    ui.new_workspace();
    ui.wait("the line saying so", |ui| ui.has(INSTALL_APP));
    ui.click(INSTALL_APP);

    assert_eq!(ui.cx.opened_url().as_deref(), Some("https://github.com/apps/ada-roundhouse/installations/new"));
    assert!(ui.has(OPEN_GIT), "and Git in Settings beside it");
}
