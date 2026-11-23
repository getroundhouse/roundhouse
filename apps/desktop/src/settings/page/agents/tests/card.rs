//! What an account's card says, in the words assistive apps are told: which day it was last used, when its login ends,
//! and every limit with what's left, whatever its row has room for.

use gpui_kit::TestAppContext;
use protocol::fixtures::account;
use protocol::model::{Account, HeldLogin, LimitWindow};

use super::super::card::card;
use super::super::words;
use super::made_up::{DAY_MS, MINUTE_MS, NOW_MS, added, holding, limit};

/// What the card of `account` says at [`NOW_MS`], with days in UTC, or `None` for no card.
fn said(cx: &mut TestAppContext, account: &Account) -> Option<String> {
    cx.update(|cx| {
        look::hold_still(NOW_MS, cx);
        let standing = account.login_agent.map(|_| words::standing(account.held_login.as_ref(), NOW_MS, cx));
        card(&holding([account]), account, standing.as_ref(), NOW_MS, cx).map(|card| card.said)
    })
}

/// An account the person added, last used yesterday, whose login ends at `ends_at_ms` a year after it was connected,
/// with two limits heard.
fn used(ends_at_ms: i64) -> Account {
    Account {
        used_at_ms: Some(NOW_MS - DAY_MS),
        held_login: Some(HeldLogin { connected_at_ms: ends_at_ms - 365 * DAY_MS, ends_at_ms: Some(ends_at_ms) }),
        limits: vec![
            limit(LimitWindow::FiveHours, 29, NOW_MS + 120 * MINUTE_MS),
            limit(LimitWindow::SevenDays, 4, NOW_MS + 3 * DAY_MS),
        ],
        ..added("a", "Work")
    }
}

#[gpui_kit::test]
fn a_card_says_when_its_account_was_last_used_when_its_login_ends_and_whats_left_of_each_limit(
    cx: &mut TestAppContext,
) {
    assert_eq!(
        said(cx, &used(NOW_MS + 345 * DAY_MS)).as_deref(),
        Some(
            "Last used yesterday. Ends Oct 12, 2027. 5h limit, 71% left, resets 5:00 PM. 7d limit, 96% left, resets \
             Wed 3:00 PM."
        ),
        "the end, though its row doesn't warn of it yet"
    );
}

#[gpui_kit::test]
fn a_card_says_every_limit_while_its_row_warns_of_its_logins_end_in_their_place(cx: &mut TestAppContext) {
    let ending = said(cx, &used(NOW_MS + 9 * DAY_MS)).unwrap();
    let ended = said(cx, &used(NOW_MS - 3 * DAY_MS)).unwrap();

    assert!(ending.starts_with("Last used yesterday. Ends in 9 days, on Nov 10, 2026. 5h limit, 71% left"), "{ending}");
    assert!(ended.starts_with("Last used yesterday. Ended on Oct 29, 2026. 5h limit, 71% left"), "{ended}");
}

#[gpui_kit::test]
fn a_card_leaves_out_what_isnt_known_and_an_account_with_nothing_to_say_has_none(cx: &mut TestAppContext) {
    let disconnected = Account { held_login: None, ..used(NOW_MS + 345 * DAY_MS) };
    let started_again = Account { limits: vec![limit(LimitWindow::FiveHours, 40, NOW_MS - 1)], ..account("b") };

    assert!(said(cx, &disconnected).unwrap().starts_with("Last used yesterday. 5h limit"), "no end, with no login");
    assert_eq!(said(cx, &account("c")).as_deref(), Some("No limits heard of yet."), "never used, as its row says");
    assert_eq!(said(cx, &started_again), None, "every limit heard has started again");
}
