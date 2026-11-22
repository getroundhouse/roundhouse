//! Signing in with GitHub from Git in Settings, against a real hub that runs GitHub's device flow with a stand-in for
//! GitHub: the code while it waits, and each way it ends, as the hub's record of it says, and cancelling it. The
//! stand-in's codes say no time between asks after a sign-in's token, so the hub asks as often as a test's hub lets
//! itself, as `hub::Timers::FOR_TESTS` says.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use hub::{GithubAnswer, GithubScript};
use protocol::model::{GithubLoginKind, GithubSignInStatus};

use super::git::{default, in_git};
use crate::support::{Stack, Ui};

/// What GitHub gives a sign-in once it's approved, which it knows as `ada`'s.
const GRANTED: &str = "gho_made_up_for_a_test";

/// GitHub, giving a code for each sign-in in turn, each answering `polls` in turn as it's asked after its token.
fn signing_in(polls: Vec<Vec<GithubAnswer>>) -> GithubScript {
    const DEVICES: [&str; 3] = ["made-up-device-1", "made-up-device-2", "made-up-device-3"];
    GithubScript {
        users: vec![(GRANTED, GithubAnswer::user(7, "ada", None))],
        codes: DEVICES.iter().map(|device| GithubAnswer::code(device, 900, 0)).collect(),
        polls: DEVICES.into_iter().zip(polls).collect(),
        ..GithubScript::default()
    }
}

fn pending() -> GithubAnswer {
    GithubAnswer::device_flow_error("authorization_pending")
}

/// What the panel says under its title, as it tells assistive apps.
fn says(ui: &mut Ui<'_>) -> String {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find("git-panel-says").label().unwrap().to_string()).unwrap()
}

/// How each sign-in the app has a record of stands. The hub keeps one that has ended until another starts, so once
/// the app has heard what a new one says, it's the only one.
fn statuses(ui: &Ui<'_>) -> Vec<GithubSignInStatus> {
    ui.view().records().github_sign_ins().map(|sign_in| sign_in.status.clone()).collect()
}

#[gpui_kit::test]
fn a_sign_in_shows_its_code_while_it_waits_and_ends_signed_in_as_the_default(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let granted = GithubAnswer::granted(GRANTED, "ghr_made_up_for_a_test");
    let _github = stack.start_hub_with_stand_in_github(signing_in(vec![vec![pending(), granted]]));
    let mut ui = in_git(cx, &stack);

    ui.click("git-connect");
    ui.click("git-sign-in");

    ui.wait("the code", |ui| ui.has("git-code"));
    assert!(ui.has("git-open-address") && ui.has("git-cancel"));
    assert!(ui.off("git-connect"), "while a panel is open there");
    ui.wait("Done", |ui| ui.has("git-close"));
    // The records have it once the hub has said so, which can come after the panel says it's done.
    ui.wait("the account", |ui| !ui.view().records().github_accounts().is_empty());
    let account = ui.view().records().github_accounts()[0].clone();
    assert_eq!(account.user.name.as_str(), "ada");
    assert_eq!(account.held_login.map(|held| held.how), Some(GithubLoginKind::SignIn));
    // Which account is the default is on the hub's settings, a record of their own, which reaches the app apart from
    // the account's, so it's waited for rather than read as soon as the account is there.
    ui.wait("the first account to be the default", |ui| default(ui) == Some(account.id.clone()));
    assert_eq!(
        says(&mut ui),
        "Signed in as ada. It's the default, so agents push as it wherever Roundhouse sets up GitHub."
    );
    assert!(!ui.has("git-sign-in-again"));

    ui.click("git-close");

    assert!(!ui.has("git-panel"));
    assert!(ui.has(format!("git-more-{}", account.id)));
}

#[gpui_kit::test]
fn a_sign_in_that_isnt_approved_says_how_it_ended_and_offers_to_sign_in_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ended = |error| vec![GithubAnswer::device_flow_error(error)];
    let polls = vec![ended("access_denied"), ended("expired_token"), ended("device_flow_disabled")];
    let _github = stack.start_hub_with_stand_in_github(signing_in(polls));
    let mut ui = in_git(cx, &stack);
    ui.click("git-connect");

    ui.click("git-sign-in");
    ui.wait("the refusal", |ui| ui.has("git-sign-in-again"));
    assert_eq!(statuses(&ui), [GithubSignInStatus::Refused]);
    assert_eq!(says(&mut ui), "You refused it on GitHub, so nothing was connected.");

    ui.click("git-sign-in-again");
    ui.wait("the code's end", |ui| statuses(ui) == [GithubSignInStatus::Ended] && ui.has("git-sign-in-again"));
    assert!(says(&mut ui).starts_with("The sign-in ended before it was approved on GitHub"));

    ui.click("git-sign-in-again");
    let failed = |ui: &mut Ui<'_>| matches!(&statuses(ui)[..], [GithubSignInStatus::Failed { .. }]);
    ui.wait("the failure", |ui| failed(ui) && ui.has("git-sign-in-again"));
    let [GithubSignInStatus::Failed { message }] = &statuses(&ui)[..] else { unreachable!() };
    assert_eq!(&says(&mut ui), message, "it says why, as the hub does");
    assert!(ui.view().records().github_accounts().is_empty(), "nothing was connected");
}

#[gpui_kit::test]
fn cancelling_a_sign_in_has_the_hub_stop_waiting_and_puts_its_panel_away(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let _github = stack.start_hub_with_stand_in_github(signing_in(vec![vec![pending()]]));
    let mut ui = in_git(cx, &stack);
    ui.click("git-connect");
    ui.click("git-sign-in");
    ui.wait("the code", |ui| ui.has("git-code"));

    ui.click("git-cancel");

    assert!(!ui.has("git-panel"));
    let hubs = || stack.client().records_list().unwrap().github_sign_ins;
    crate::support::wait_until("the hub to cancel it", || {
        hubs().first().is_some_and(|sign_in| sign_in.status == GithubSignInStatus::Cancelled)
    });
    ui.wait("the app to hear it", |ui| statuses(ui) == [GithubSignInStatus::Cancelled]);
    assert!(!ui.has("git-panel"), "one that ended shows only in the panel that showed it waiting");
    assert!(ui.on("git-connect"));
}
