//! What Git says of whether agents can push, as the hub says what it sends every worker for the default, against a real
//! hub that calls a stand-in for GitHub: an installation whose next token GitHub won't make past its last's end, which
//! its record can't say, a token's default ended, and a sign-in being renewed. Every token is
//! plainly made up, and each ends as GitHub gives it, so nothing waits for a clock.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use hub::{GithubAnswer, GithubScript};
use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{SentCredential, UnusableLogin};

use super::git::{connected, in_git};
use super::git_app::{CODE, making, on_adalabs};
use crate::support::{Stack, Ui};

/// What the box at the top of Added says, as it tells assistive apps.
fn box_says(ui: &mut Ui<'_>) -> String {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find("git-blocked").label().unwrap().to_string()).unwrap()
}

/// What the hub's settings say it sends every worker for the default, as the app has them.
fn sent(ui: &Ui<'_>) -> Option<SentCredential> {
    ui.view().records().hub_settings()?.github.as_ref()?.sent.clone()
}

/// Why the hub blocks every worker, as the app has the hub's settings say, if it does.
fn blocked_for(ui: &Ui<'_>) -> Option<UnusableLogin> {
    match sent(ui)? {
        SentCredential::Blocked { why, .. } => Some(why),
        _ => None,
    }
}

/// Its record says nothing of when an installation's token ends, so only the hub can say the default blocks.
#[gpui_kit::test]
fn a_default_installation_whose_next_token_github_wont_make_says_so_in_the_box_and_its_row(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ended = GithubAnswer::installation_token("ghs_made_up_and_ended", "2020-01-01T00:00:00Z");
    let script = GithubScript {
        minted: vec![(70, vec![ended, GithubAnswer::status(500, Vec::new())])],
        ..making(vec![on_adalabs()])
    };
    let _github = stack.start_hub_with_stand_in_github(script);
    stack.client().github_app_make(GithubManifestCode::new(CODE)).unwrap();
    let mut ui = in_git(cx, &stack);
    let bot = ui.view().records().github_accounts()[0].clone();
    let row = format!("git-account-{}", bot.id);

    ui.wait("agents blocked", |ui| ui.has("git-blocked") && ui.shows_label(&row, "Renewing"));

    assert_eq!(blocked_for(&ui), Some(UnusableLogin::Renewing));
    assert!(bot.held_login.unwrap().ends_at_ms.is_none(), "its record says nothing of its token's end");
    let says = box_says(&mut ui);
    let lead = "Roundhouse can't use its GitHub account. The token of ada-roundhouse[bot], the default account, \
                installed on adalabs, has ended, and Roundhouse is making another.";
    assert!(says.starts_with(lead), "{says}");
    assert_eq!(says.matches("can't push").count(), 1, "the reason says it once: {says}");
    assert!(!ui.shows_label(&row, "Installed on adalabs"));
}

#[gpui_kit::test]
fn a_tokens_default_that_has_ended_says_so_in_the_box_and_its_row(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ended = GithubAnswer::user(7, "ada", Some("2020-06-15 12:00:00 UTC"));
    let _github = stack.start_hub_with_stand_in_github(GithubScript {
        users: vec![(super::git::TOKEN, ended)],
        ..GithubScript::default()
    });
    let account = connected(&stack, super::git::TOKEN);
    let mut ui = in_git(cx, &stack);
    let row = format!("git-account-{account}");

    ui.wait("agents blocked", |ui| ui.has("git-blocked") && ui.shows_label(&row, "Ended Jun 15, 2020"));

    assert_eq!(blocked_for(&ui), Some(UnusableLogin::Ended));
    let says = box_says(&mut ui);
    assert!(says.contains("The token of ada, the default account, has ended") && says.contains("Renew it"), "{says}");
}

/// As after a Mac wakes past a sign-in's token's end, before the hub's next check has renewed it.
#[gpui_kit::test]
fn a_default_signed_in_whose_token_has_ended_says_its_being_renewed_in_amber(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let granted = serde_json::json!({
        "access_token": "gho_made_up_and_ended",
        "token_type": "bearer",
        "expires_in": 0,
        "refresh_token": "ghr_made_up_for_a_test",
        "refresh_token_expires_in": 15_638_400,
    });
    let granted = GithubAnswer::With { status: 200, headers: Vec::new(), body: granted.to_string() };
    let _github = stack.start_hub_with_stand_in_github(GithubScript {
        users: vec![("gho_made_up_and_ended", GithubAnswer::user(7, "ada", None))],
        codes: vec![GithubAnswer::code("made-up-device", 900, 1)],
        polls: vec![("made-up-device", vec![granted])],
        // So a check that comes meanwhile renews nothing.
        renewals: vec![("ghr_made_up_for_a_test", vec![GithubAnswer::status(503, Vec::new())])],
        ..GithubScript::default()
    });
    let mut ui = in_git(cx, &stack);
    ui.click("git-connect");
    ui.click("git-sign-in");
    ui.wait("Done", |ui| ui.has("git-close"));
    ui.click("git-close");
    let account = ui.view().records().github_accounts()[0].id.clone();

    ui.wait("agents blocked", |ui| {
        ui.has("git-blocked") && ui.shows_label(&format!("git-account-{account}"), "Renewing")
    });

    assert_eq!(blocked_for(&ui), Some(UnusableLogin::Renewing));
    assert!(box_says(&mut ui).contains("ada, the default account, has ended, and Roundhouse is renewing it"));
}
