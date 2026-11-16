//! What an account's row says of its login, what stops the default being used, what a found login's row says, the
//! order Added lists accounts in, and when the page is drawn again.

use gpui_kit::TestAppContext;
use protocol::fixtures::github_account;
use protocol::model::{GhLogin, GhLoginWorks, GithubAccount, GithubLoginKind, HeldGithubLogin};

use super::super::added::listed;
use super::super::git_page::changes_at_ms;
use super::super::words::{self, Standing};
use super::made_up::{FEB_3_MS, HOUR_MS, NOW_MS, OCT_29_MS, held, named};

/// How `held` stands at [`NOW_MS`], with days in UTC.
fn standing(cx: &mut TestAppContext, held: Option<HeldGithubLogin>) -> Standing {
    cx.update(|cx| {
        look::hold_still(NOW_MS, cx);
        Standing::of(held.as_ref(), NOW_MS, cx)
    })
}

#[gpui_kit::test]
fn a_tokens_row_says_when_it_ends_and_a_sign_ins_says_nothing_of_it_since_the_hub_renews_it(cx: &mut TestAppContext) {
    let token = |ends_at_ms| Some(held(GithubLoginKind::Token, ends_at_ms, false));
    let sign_in = |ends_at_ms, needs_sign_in| Some(held(GithubLoginKind::SignIn, ends_at_ms, needs_sign_in));

    let ends = standing(cx, token(Some(FEB_3_MS)));
    assert_eq!(ends.said().as_deref(), Some("Ends Feb 3, 2027"));
    assert!(ends.works());
    assert_eq!(standing(cx, token(None)).said(), None, "one that never ends says nothing");
    assert_eq!(standing(cx, sign_in(Some(NOW_MS + 6 * HOUR_MS), false)).said(), None);
    let ended = standing(cx, token(Some(OCT_29_MS)));
    assert_eq!(ended.said().as_deref(), Some("Ended Oct 29, 2026"));
    assert!(!ended.works());
    assert_eq!(standing(cx, token(Some(NOW_MS))), Standing::Ended { on: "Nov 1, 2026".to_string() }, "from then on");
    assert_eq!(standing(cx, sign_in(Some(NOW_MS - HOUR_MS), false)), Standing::Renewing, "the hub renews it");
    assert_eq!(standing(cx, sign_in(Some(NOW_MS + HOUR_MS), true)), Standing::SignInAgain, "it can't be renewed");
    assert_eq!(standing(cx, None), Standing::Disconnected);
    assert_eq!(Standing::Renewing.said().as_deref(), Some("Renewing"));
    assert_eq!(Standing::SignInAgain.said().as_deref(), Some("Needs signing in again"));
    assert!(![Standing::Renewing, Standing::SignInAgain, Standing::Disconnected].iter().any(Standing::works));
}

#[test]
fn what_stops_the_default_being_used_says_what_to_do_and_a_renewing_one_that_its_renewing() {
    let blocked = |standing: Standing| standing.blocked("ada");

    assert_eq!(blocked(Standing::Works { ends: None }), None);
    assert_eq!(
        blocked(Standing::Ended { on: "Oct 29, 2026".to_string() }).as_deref(),
        Some(
            "The token of the default, ada, ended Oct 29, 2026. Renew it from its menu, or make another account the default."
        )
    );
    assert!(blocked(Standing::Renewing).unwrap().contains("Roundhouse is renewing it"));
    assert_eq!(
        blocked(Standing::SignInAgain).as_deref(),
        Some(
            "The default, ada, needs signing in with GitHub again. Sign in again from its menu, or make another \
             account the default."
        )
    );
    assert!(blocked(Standing::Disconnected).unwrap().contains("was disconnected. Connect it again from its menu"));
}

#[test]
fn a_found_login_says_who_whether_github_took_it_and_what_to_do_when_theres_none() {
    let logged_in = |works| GhLogin::LoggedIn { username: "ada".into(), works };

    assert_eq!(words::found(&logged_in(GhLoginWorks::Yes)), ("ada".to_string(), Some(("Works", false))));
    assert_eq!(
        words::found(&logged_in(GhLoginWorks::Refused)),
        ("ada".to_string(), Some(("GitHub refused its token", true))),
        "in red"
    );
    assert_eq!(words::found(&logged_in(GhLoginWorks::CouldntTell)).1, Some(("Couldn't tell if it works", false)));
    assert_eq!(words::found(&logged_in(GhLoginWorks::Unknown)).1, None, "a newer hub's answer says nothing");
    assert_eq!(
        words::found(&GhLogin::LoggedOut),
        ("Not logged in".to_string(), Some(("Run gh auth login there", false)))
    );
    assert_eq!(words::found(&GhLogin::NotInstalled), ("No gh".to_string(), Some(("Install gh there", false))));
    assert_eq!(words::found_on(&"local".into()), format!("gh on this {}", crate::names::MACHINE));
    assert_eq!(words::found_on(&"incus".into()), "gh on incus");
}

#[test]
fn added_lists_the_default_first_then_those_held_then_those_disconnected_each_as_client_core_does() {
    let accounts =
        [named("g1", "ada", false), named("g2", "bea", true), named("g3", "cy", false), named("g4", "dee", false)];
    let ids = |listed: Vec<&GithubAccount>| listed.iter().map(|account| account.id.to_string()).collect::<Vec<_>>();

    assert_eq!(ids(listed(&accounts, Some(&"g3".into()))), ["g3", "g1", "g4", "g2"]);
    assert_eq!(ids(listed(&accounts, Some(&"g2".into()))), ["g2", "g1", "g3", "g4"], "a disconnected default too");
    assert_eq!(ids(listed(&accounts, None)), ["g1", "g3", "g4", "g2"]);
}

#[test]
fn the_page_is_drawn_again_as_a_login_it_shows_ends() {
    let ending = |id, ends_at_ms| GithubAccount {
        held_login: Some(held(GithubLoginKind::Token, Some(ends_at_ms), false)),
        ..github_account(id)
    };

    let accounts = [ending("a", NOW_MS + 2 * HOUR_MS), ending("b", NOW_MS + HOUR_MS), ending("c", NOW_MS - 1)];
    assert_eq!(changes_at_ms(&accounts, NOW_MS), Some(NOW_MS + HOUR_MS));
    assert_eq!(changes_at_ms(&[ending("c", NOW_MS)], NOW_MS), None, "once it has ended, nothing changes");
    assert_eq!(changes_at_ms(&[github_account("d")], NOW_MS), None, "nor for one that never ends");
}
