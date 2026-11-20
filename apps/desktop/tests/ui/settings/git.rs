//! Git in Settings, against a real hub that calls a stand-in for GitHub: connecting a token, which becomes the
//! default, one GitHub refuses, disconnecting, which asks first and leaves the default blocked, renewing an account's
//! token and connecting it again from its menu, and choosing the default from an account's menu.
//! Every token is plainly made up.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::{SharedString, TestAppContext};
use hub::{GithubAnswer, GithubScript};
use protocol::model::{GithubAccount, GithubAccountId, GithubToken};

use super::open_settings;
use crate::support::{Stack, Ui, files_holding};

pub(super) const TOKEN: &str = "github_pat_made_up_for_a_test";
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
    ui.wait("Connect account to be on", |ui| ui.on("git-connect"));
    ui
}

/// The default GitHub account, as the app shows it, or `None` for none.
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
pub(super) fn connected(stack: &Stack, token: &str) -> GithubAccountId {
    stack.client().github_account_connect(GithubToken::new(token), None).unwrap().id
}

pub(super) fn more(account: &GithubAccountId) -> SharedString {
    format!("git-more-{account}").into()
}

/// One walk through connecting a token: first one GitHub refuses, then one it knows, in the same panel.
#[gpui_kit::test]
fn a_token_connected_here_becomes_the_default_only_the_hub_keeps_it_and_one_github_refuses_says_why(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(knowing_two());
    let mut ui = in_git(cx, &stack);
    assert!(ui.has("git-none-added"));

    ui.click("git-connect");
    ui.click("git-use-token");
    assert!(ui.off("git-panel-connect"), "until there's a token");
    ui.input("github_pat_github_never_made");
    ui.press("enter");
    ui.wait("GitHub's refusal", |ui| ui.has("git-refused"));
    assert!(ui.has("git-panel"), "a token GitHub refuses: it stays open, to paste another");
    assert!(ui.off("git-panel-connect"), "a token GitHub refuses: its field is empty");
    assert!(ui.view().records().github_accounts().is_empty(), "a token GitHub refuses: nothing was connected");
    assert!(the_hubs(&stack).is_empty(), "a token GitHub refuses: the hub has nothing either");

    ui.input(TOKEN);
    ui.click("git-panel-connect");

    ui.wait("the account", |ui| !ui.view().records().github_accounts().is_empty() && !ui.has("git-panel"));
    let account = ui.view().records().github_accounts()[0].clone();
    assert_eq!(account.user.name.as_str(), "ada");
    // Which account is the default is on the hub's settings, a record of their own, which reaches the app apart from
    // the account's, so it's waited for rather than read as soon as the account is there.
    ui.wait("the first account to be the default", |ui| default(ui) == Some(account.id.clone()));
    assert!(ui.has(more(&account.id)), "it's listed, with its menu");
    assert!(!ui.has("git-none-added") && !ui.has("git-blocked"));
    let everything = stack.dir.path();
    assert_eq!(files_holding(everything, TOKEN, "github-logins"), Vec::<PathBuf>::new(), "nothing else has it");
    assert_eq!(files_holding(everything, TOKEN, "").len(), 1, "the hub's GitHub logins do");
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
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("disconnect-github-account-keep");
    ui.wait_for_every_call();
    assert!(the_hubs(&stack)[0].held_login.is_some(), "nothing is forgotten without asking");

    ui.click(more(&account));
    ui.click_menu_item("Disconnect");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("disconnect-github-account-act");

    ui.wait("agents blocked", |ui| ui.has("git-blocked"));
    assert_eq!(the_hubs(&stack)[0].held_login, None);
    assert_eq!(the_hubs_default(&stack), Some(account.clone()), "it stays the default");
    assert_eq!(default(&ui), Some(account));
    let logins = stack.hub.paths.data_dir.join("github-logins");
    assert!(files_holding(&logins, TOKEN, "").is_empty(), "the hub forgot the token");
}

/// Another token of ada's, which GitHub knows as hers.
const ADAS_NEXT: &str = "github_pat_ada_made_up_next";

/// A token for the account itself, rather than a new account for the same user, which would leave the default with
/// no login, and blocked.
#[gpui_kit::test]
fn renew_and_connect_again_from_an_accounts_menu_put_the_token_in_place_of_the_accounts_own(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ada = GithubAnswer::user(7, "ada", None);
    let script = GithubScript { users: vec![(TOKEN, ada.clone()), (ADAS_NEXT, ada)], ..GithubScript::default() };
    let _github = stack.start_hub_with_stand_in_github(script);
    let account = connected(&stack, TOKEN);
    let mut ui = in_git(cx, &stack);
    ui.wait("its row", |ui| ui.has(more(&account)));
    let logins = stack.hub.paths.data_dir.join("github-logins");

    ui.click(more(&account));
    ui.click_menu_item("Renew");
    ui.input(ADAS_NEXT);
    ui.press("enter");
    ui.wait("the new login", |ui| !ui.has("git-panel"));
    let ids = |accounts: Vec<GithubAccount>| accounts.into_iter().map(|account| account.id).collect::<Vec<_>>();
    let renewed = (ids(the_hubs(&stack)), files_holding(&logins, TOKEN, "").len());
    stack.client().github_account_disconnect(&account).unwrap();
    // The hub says agents are blocked apart from saying the account has no login, and the menu offers to connect it
    // again by the account's own record, so the test waits for both.
    ui.wait("agents blocked, and the account without a login", |ui| {
        ui.has("git-blocked") && ui.view().records().github_accounts().iter().all(|a| a.held_login.is_none())
    });
    ui.click(more(&account));
    ui.click_menu_item("Connect again");
    ui.input(TOKEN);
    ui.press("enter");

    ui.wait("the login again", |ui| !ui.has("git-panel"));
    assert_eq!(renewed, (vec![account.clone()], 0), "renewed in place, and the token it had forgotten");
    assert_eq!(ids(the_hubs(&stack)), std::slice::from_ref(&account), "connected again in place");
    assert!(the_hubs(&stack)[0].held_login.is_some());
    assert_eq!(the_hubs_default(&stack), Some(account));
    assert_eq!(files_holding(&logins, TOKEN, "").len(), 1);
    ui.wait("agents unblocked", |ui| !ui.has("git-blocked"));
}

#[gpui_kit::test]
fn the_default_is_chosen_from_an_accounts_menu(cx: &mut TestAppContext) {
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
}
