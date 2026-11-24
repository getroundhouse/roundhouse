//! What an account's row says of its login, as its record says and, for the default, as the hub says what workers are
//! sent, what a found login's row says, the order Added lists accounts in, and when the page is drawn again.

use gpui_kit::TestAppContext;
use protocol::fixtures::github_account;
use protocol::model::{GithubAccount, GithubLoginKind, HeldGithubLogin, SentCredential, UnusableLogin};

use super::super::added::listed;
use super::super::git_page::changes_at_ms;
use super::super::words::Standing;
use super::made_up::{FEB_3_MS, HOUR_MS, NOW_MS, OCT_29_MS, held, named, records};

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

/// The default's row says what the hub says of it, whatever its record says at the page's moment, but for when a
/// token ends, which only its record says.
#[gpui_kit::test]
fn the_defaults_row_says_how_it_stands_as_the_hub_says_what_workers_are_sent(cx: &mut TestAppContext) {
    let token = held(GithubLoginKind::Token, Some(OCT_29_MS), false);
    let standing = |sent: SentCredential, held: Option<HeldGithubLogin>| {
        cx.update(|cx| {
            look::hold_still(NOW_MS, cx);
            Standing::sent(&sent, held.as_ref(), cx)
        })
    };
    let blocked = |why| SentCredential::Blocked { account: "g".into(), why, reason: "Made up.".to_string() };

    let sent_anyway = standing(SentCredential::Account { account: "g".into() }, Some(token));
    assert_eq!(sent_anyway.said().as_deref(), Some("Ends Oct 29, 2026"), "the hub hasn't blocked it yet");
    assert!(sent_anyway.works());
    let ended = standing(blocked(UnusableLogin::Ended), Some(token));
    assert_eq!(ended.said().as_deref(), Some("Ended Oct 29, 2026"));
    let fine_by_its_record = Some(held(GithubLoginKind::App, None, false));
    assert_eq!(standing(blocked(UnusableLogin::Renewing), fine_by_its_record), Standing::Renewing);
    assert_eq!(standing(blocked(UnusableLogin::SignInAgain), None), Standing::SignInAgain);
    assert_eq!(standing(blocked(UnusableLogin::Disconnected), fine_by_its_record), Standing::Disconnected);
    for why in [UnusableLogin::Newer, UnusableLogin::Unreadable, UnusableLogin::Unknown] {
        let cant = standing(blocked(why), fine_by_its_record);
        assert_eq!((cant.said().as_deref(), cant.works()), (Some("Can't be used"), false), "{why:?}");
    }
    assert_eq!(standing(SentCredential::Unknown, fine_by_its_record), Standing::CantBeUsed, "a newer hub's word");
    assert_eq!(Standing::Ended { on: String::new() }.said().as_deref(), Some("Ended"), "with no end on its record");
}

/// As the hub lists them, which is in neither client-core's order nor their IDs', and client-core keeps them by
/// username, in any case, so an order by ID, or by name with capitals first, differs from it.
#[test]
fn added_lists_the_default_first_then_those_held_then_those_disconnected_each_as_client_core_does() {
    let hubs =
        vec![named("g1", "dee", false), named("g2", "bea", true), named("g3", "Cy", false), named("g4", "ada", false)];
    let records = records(hubs, Vec::new(), None);
    let accounts = records.github_accounts();
    let ids = |listed: Vec<&GithubAccount>| listed.iter().map(|account| account.id.to_string()).collect::<Vec<_>>();

    assert_eq!(ids(accounts.clone()), ["g4", "g2", "g3", "g1"], "ada, bea, Cy and dee");
    assert_eq!(ids(listed(accounts.clone(), Some(&"g1".into()))), ["g1", "g4", "g3", "g2"]);
    assert_eq!(
        ids(listed(accounts.clone(), Some(&"g2".into()))),
        ["g2", "g4", "g3", "g1"],
        "a disconnected default too"
    );
    assert_eq!(ids(listed(accounts, None)), ["g4", "g3", "g1", "g2"]);
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
