//! How a sign-in's panel shows it, from the hub's record of it, and what it offers and says.

use client_core::ClientError;
use gpui_kit::TestAppContext;
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::github_sign_in;
use protocol::model::GithubSignInStatus;

use super::super::sign_in::SignInPanel;
use super::super::sign_in_words::{self, Stands};
use super::made_up::{ended, named, panel, records};

#[gpui_kit::test]
fn a_sign_ins_panel_shows_it_as_the_hubs_record_of_it_says(cx: &mut TestAppContext) {
    let approved = ended("s2", GithubSignInStatus::Approved { account: "g1".into() });
    let held = records(vec![named("g1", "ada", false)], vec![github_sign_in("s1"), approved], Some("g1"));
    let github = held.hub_settings().and_then(|settings| settings.github.as_ref());
    cx.update(|cx| {
        look::hold_still(0, cx);
        let stands = |panel: &SignInPanel| format!("{:?}", Stands::of(panel, &held, github, cx));

        assert_eq!(stands(&panel("s0", true, None)), "Starting", "until the hub answers its start");
        assert_eq!(stands(&panel("s0", false, None)), "Gone", "once the hub has none");
        assert_eq!(stands(&panel("s0", false, Some("Couldn't"))), "NotStarted(\"Couldn't\")");
        let shows_s1 = panel("s1", false, None);
        let waiting = Stands::of(&shows_s1, &held, github, cx);
        let Stands::Waiting { code, address, until } = &waiting else { panic!("{waiting:?}") };
        assert_eq!(
            (code.as_str(), *address, until.as_deref()),
            ("WDJB-MJHT", "https://github.com/login/device", Some("12:15 AM"))
        );
        assert_eq!(stands(&panel("s2", false, None)), "Approved { name: Some(\"ada\"), default: true }");
    });
}

#[test]
fn a_sign_ins_panel_offers_cancel_while_the_hub_may_wait_and_sign_in_again_where_another_try_could_differ() {
    let failed = Stands::Failed("GitHub said no.");
    let ended = [Stands::Refused, Stands::Ended, failed, Stands::NotStarted("Couldn't")];
    let neither = [Stands::Cancelled, Stands::Gone, Stands::Unknown, Stands::Approved { name: None, default: false }];

    assert!(Stands::Starting.may_wait() && !Stands::Starting.may_try_again());
    assert!(ended.iter().all(|stands| stands.may_try_again() && !stands.may_wait()));
    assert!(neither.iter().all(|stands| !stands.may_try_again() && !stands.may_wait()));
    assert_eq!(Stands::Approved { name: Some("ada".into()), default: false }.lead(None), "Signed in as ada.");
    assert_eq!(Stands::Approved { name: None, default: true }.title(), "Signed in with GitHub");
    assert_eq!(Stands::Refused.title(), "Sign in with GitHub");
    assert_eq!(ended[2].lead(None), "GitHub said no.", "it says why, as the hub does");
}

#[test]
fn a_waiting_sign_in_started_from_an_accounts_row_says_to_sign_in_as_that_account() {
    let code = "WDJB-MJHT".into();
    let waiting = Stands::Waiting { code: &code, address: "https://github.com/login/device", until: None };

    assert!(waiting.lead(Some("ada")).starts_with("Enter this code on GitHub, signed in as ada,"));
    assert!(waiting.lead(None).starts_with("Enter this code on GitHub, in a browser"));
    assert_eq!(
        sign_in_words::waiting(Some("3:15 PM")),
        "Waiting for you to approve it on GitHub. The code works until 3:15 PM."
    );
    assert_eq!(sign_in_words::waiting(None), "Waiting for you to approve it on GitHub.");
}

#[test]
fn a_sign_in_the_hub_couldnt_start_says_why_for_a_person() {
    let refused = |code, message: &str| sign_in_words::not_started(&ClientError::Api(ApiError::new(code, message)));

    assert!(
        refused(ErrorCode::Conflict, "four waiting").starts_with("As many sign-ins as the hub allows are waiting.")
    );
    assert_eq!(
        refused(ErrorCode::Unavailable, "GitHub didn't answer"),
        "Couldn't start signing in: GitHub didn't answer"
    );
    assert_eq!(
        sign_in_words::not_started(&ClientError::Disconnected),
        "Couldn't start signing in: the connection to the hub was lost"
    );
}
