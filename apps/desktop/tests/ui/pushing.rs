//! What a workspace says when agents can't push to its repository on GitHub as the default GitHub account, against a
//! real hub that checks with a stand-in for GitHub. Every token is plainly made up.

#![cfg(test)]

use desktop::workspace_pane::OPEN_GIT;
use gpui_kit::TestAppContext;
use hub::{GithubAnswer, GithubScript};
use protocol::model::GithubToken;

use crate::support::{Stack, Ui, git};

const TOKEN: &str = "github_pat_made_up_for_a_test";

/// From the hub's check to the line in the workspace, and from its button to where it says what to do. The
/// repository fetches from itself and pushes to `cli/cli` on GitHub, which refuses `ada`, so nothing reaches GitHub.
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
    git(&repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    git(&repo, &["remote", "set-url", "--push", "origin", "git@github.com:cli/cli.git"]);
    let client = stack.client();
    client.github_account_connect(GithubToken::new(TOKEN), None).unwrap();
    client.repo_add(repo.to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);

    ui.new_workspace();
    ui.wait("the line saying so", |ui| ui.has(OPEN_GIT));
    ui.click(OPEN_GIT);

    ui.wait("Git in Settings", |ui| ui.has("git-connect"));
    assert!(ui.has("settings-back"), "in Settings, with the way back");
}
