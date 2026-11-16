//! Git in Settings, against a real hub that calls a stand-in for GitHub: connecting a token, which becomes the
//! default, one GitHub refuses, disconnecting, which asks first and leaves the default blocked, and choosing the
//! default, an account or each machine's own login. Every token is plainly made up.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::{SharedString, TestAppContext};
use hub::{GithubAnswer, GithubScript};
use protocol::model::{GithubAccount, GithubAccountId, GithubToken};

use super::open_settings;
use crate::support::{Stack, Ui, files_holding};

const TOKEN: &str = "github_pat_made_up_for_a_test";
const ANOTHER: &str = "github_pat_another_made_up";

/// GitHub, which knows `ada` by [`TOKEN`] and `grace` by [`ANOTHER`], and refuses any other token.
pub(super) fn knowing_two() -> GithubScript {
    GithubScript {
        users: vec![(TOKEN, GithubAnswer::user(7, "ada", None)), (ANOTHER, GithubAnswer::user(8, "grace", None))],
        ..GithubScript::default()
    }
}

/// Opens Git in Settings, once the hub's settings have said it keeps GitHub accounts, so Connect account is on.
pub(super) fn in_git<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    open_settings(&mut ui);
    ui.click("settings-page-git");
    ui.wait("Connect account to be on", |ui| ui.has("git-connect") && !ui.disabled("git-connect"));
    ui
}

/// The default GitHub account, as the app shows it, or `None` for each machine's own login.
pub(super) fn default(ui: &Ui<'_>) -> Option<GithubAccountId> {
    ui.view().records().hub_settings()?.github.as_ref()?.account.clone()
}

/// The default GitHub account, as the hub says when asked, rather than as the app shows it.
fn the_hubs_default(stack: &Stack) -> Option<GithubAccountId> {
    stack.client().records_list().unwrap().hub_settings.unwrap().github.unwrap().account
}

/// Every GitHub account the hub has, as it says when asked.
fn the_hubs(stack: &Stack) -> Vec<GithubAccount> {
    stack.client().records_list().unwrap().github_accounts
}

/// An account whose token the hub was given from elsewhere.
fn connected(stack: &Stack, token: &str) -> GithubAccountId {
    stack.client().github_account_connect(GithubToken::new(token), None).unwrap().id
}

fn more(account: &GithubAccountId) -> SharedString {
    format!("git-more-{account}").into()
}

#[gpui_kit::test]
fn a_token_connected_here_becomes_the_default_and_only_the_hub_keeps_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(knowing_two());
    let mut ui = in_git(cx, &stack);
    assert!(ui.has("git-none-added"));
    assert!(ui.has("git-none-found"), "with no workspace, no machine has been asked");

    ui.click("git-connect");
    ui.click("git-use-token");
    assert!(ui.disabled("git-panel-connect"), "until there's a token");
    ui.input(TOKEN);
    ui.click("git-panel-connect");

    ui.wait("the account", |ui| !ui.view().records().github_accounts().is_empty() && !ui.has("git-panel"));
    let account = ui.view().records().github_accounts()[0].clone();
    assert_eq!(account.user.name.as_str(), "ada");
    assert_eq!(default(&ui), Some(account.id.clone()), "the first account is the default");
    assert!(ui.has(more(&account.id)), "it's listed, with its menu");
    assert!(!ui.has("git-none-added") && !ui.has("git-blocked"));
    let everything = stack.dir.path();
    assert_eq!(files_holding(everything, TOKEN, "github-logins"), Vec::<PathBuf>::new(), "nothing else has it");
    assert_eq!(files_holding(everything, TOKEN, "").len(), 1, "the hub's GitHub logins do");
}

#[gpui_kit::test]
fn a_token_github_refuses_says_why_and_is_gone_from_its_field(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(knowing_two());
    let mut ui = in_git(cx, &stack);
    ui.click("git-connect");
    ui.click("git-use-token");
    ui.input("github_pat_github_never_made");

    ui.press("enter");

    ui.wait("the refusal", |ui| ui.has("git-refused"));
    assert!(ui.has("git-panel"), "it stays open, to paste another");
    assert!(ui.disabled("git-panel-connect"), "its field is empty");
    assert!(ui.view().records().github_accounts().is_empty());
    assert!(the_hubs(&stack).is_empty());
}

#[gpui_kit::test]
fn disconnecting_asks_first_and_the_default_stays_the_default_with_agents_blocked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(knowing_two());
    let account = connected(&stack, TOKEN);
    let mut ui = in_git(cx, &stack);
    ui.wait("its row", |ui| ui.has(more(&account)));

    ui.click(more(&account));
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Cancel");
    ui.wait_for_every_call();
    assert!(the_hubs(&stack)[0].held_login.is_some(), "nothing is forgotten without asking");

    ui.click(more(&account));
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.cx.has_pending_prompt());
    ui.cx.simulate_prompt_answer("Disconnect");

    ui.wait("agents blocked", |ui| ui.has("git-blocked"));
    assert_eq!(the_hubs(&stack)[0].held_login, None);
    assert_eq!(the_hubs_default(&stack), Some(account.clone()), "it stays the default");
    assert_eq!(default(&ui), Some(account));
    let logins = stack.hub.paths.data_dir.join("github-logins");
    assert!(files_holding(&logins, TOKEN, "").is_empty(), "the hub forgot the token");
}

#[gpui_kit::test]
fn the_default_is_chosen_from_an_accounts_menu_or_as_each_machines_own_by_found(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(knowing_two());
    let ada = connected(&stack, TOKEN);
    let grace = connected(&stack, ANOTHER);
    let mut ui = in_git(cx, &stack);
    ui.wait("their rows", |ui| ui.has(more(&grace)));
    assert_eq!(default(&ui), Some(ada));

    ui.click(more(&grace));
    ui.click_menu_item("Make default");

    ui.wait("grace the default", |ui| default(ui) == Some(grace.clone()));
    assert_eq!(the_hubs_default(&stack), Some(grace));

    ui.click("git-machines-own-default");

    ui.wait("each machine's own the default", |ui| default(ui).is_none() && !ui.has("git-machines-own-default"));
    assert_eq!(the_hubs_default(&stack), None);
}
