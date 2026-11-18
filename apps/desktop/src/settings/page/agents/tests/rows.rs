//! What an account's row says: how its login was made, then what else, in each state its login can be in, the default's
//! badge, the order the lists are in, what's said under a row, and when the rows are drawn again.

use gpui_kit::{AppContext as _, ScrollHandle, TestAppContext};
use protocol::fixtures::account;
use protocol::model::{Account, AccountDefault, AccountId, AgentKind, HeldLogin, LimitWindow};

use super::super::super::account_row::Tone;
use super::super::kept::Kept;
use super::super::row::changes_at_ms;
use super::super::words::{self, Says};
use super::super::{Drawing, listed};
use super::made_up::{DAY_MS, MINUTE_MS, NOW_MS, added, claude, limit};
use client_core::Agents;

/// What the row of `account` says after how its login was made, at [`NOW_MS`], with days in UTC.
fn says(cx: &mut TestAppContext, account: &Account) -> Says {
    cx.update(|cx| {
        look::hold_still(NOW_MS, cx);
        let standing = account.login_agent.map(|_| words::standing(account.held_login.as_ref(), NOW_MS, cx));
        words::says(account, standing.as_ref(), NOW_MS)
    })
}

/// A login the hub holds that lasts a year and ends at `ends_at_ms`.
fn ending(ends_at_ms: i64) -> HeldLogin {
    HeldLogin { connected_at_ms: ends_at_ms - 365 * DAY_MS, ends_at_ms: Some(ends_at_ms) }
}

fn limited(account: Account) -> Account {
    let limits = vec![
        limit(LimitWindow::FiveHours, 29, NOW_MS + 60 * MINUTE_MS),
        limit(LimitWindow::SevenDays, 4, NOW_MS + DAY_MS),
    ];
    Account { limits, ..account }
}

#[test]
fn how_a_login_was_made_is_its_kind_for_one_added_then_its_plan_and_how_its_logged_in() {
    let agents = Agents::new(vec![claude(None)]);
    let found = Account { plan: Some("Max".to_string()), login: Some("Claude account".to_string()), ..account("f") };

    assert_eq!(words::how(&found, &agents).as_deref(), Some("Max · Claude account"));
    assert_eq!(words::how(&added("a", "Work"), &agents).as_deref(), Some("Claude Code"));
    let with_a_plan = Account { plan: Some("Max".to_string()), ..added("a", "Work") };
    assert_eq!(words::how(&with_a_plan, &agents).as_deref(), Some("Claude Code · Max"));
    assert_eq!(words::how(&account("f"), &agents), None, "a record that says nothing of it");
    assert_eq!(words::how(&added("a", "Work"), &Agents::default()), None, "before the hub has described Claude");
}

#[gpui_kit::test]
fn a_login_that_needs_seeing_to_is_said_before_its_limits(cx: &mut TestAppContext) {
    let soon = Account { held_login: Some(ending(NOW_MS + 9 * DAY_MS)), ..limited(added("a", "Personal")) };
    let ended = Account { held_login: Some(ending(NOW_MS - 3 * DAY_MS)), ..limited(added("a", "Side project")) };
    let gone = Account { held_login: None, ..limited(added("a", "Contractor")) };

    assert_eq!(says(cx, &soon), Says::Standing("Ends in 9 days, on Nov 10, 2026".to_string(), Tone::Warning));
    assert_eq!(says(cx, &ended), Says::Standing("Ended on Oct 29, 2026".to_string(), Tone::Problem));
    assert_eq!(says(cx, &gone), Says::Standing("Disconnected".to_string(), Tone::Plain));
}

#[gpui_kit::test]
fn otherwise_a_row_says_whats_left_of_its_limits_or_that_none_has_been_heard(cx: &mut TestAppContext) {
    let far_off = Account { held_login: Some(ending(NOW_MS + 300 * DAY_MS)), ..added("a", "Work") };
    let found = limited(account("f"));

    let left = Says::Limits(vec![("5h".into(), 71, None), ("7d".into(), 96, None)]);
    assert_eq!(says(cx, &found), left, "a login found where agents ran");
    assert_eq!(says(cx, &limited(far_off.clone())), left, "and one held that ends after it warns");
    assert_eq!(says(cx, &far_off), Says::NoLimitsYet);
    let started_again = Account { limits: vec![limit(LimitWindow::FiveHours, 29, NOW_MS)], ..far_off };
    assert_eq!(says(cx, &started_again), Says::Nothing, "every limit heard has started again");
    assert_eq!(
        words::limits_left(&[("5h".into(), 71, None), ("7d".into(), 96, None)]),
        "5h limit, 71% left, 7d limit, 96% left"
    );
}

#[gpui_kit::test]
fn the_default_is_the_account_the_hubs_settings_name_for_its_kind(cx: &mut TestAppContext) {
    let kept = cx.new(|_| Kept::default());
    let agents = Agents::new(vec![claude(None)]);
    let scroll = ScrollHandle::new();
    let chosen = [AccountDefault { agent: AgentKind::Claude, account: "a2".into() }];
    let drawing = |defaults| Drawing { kept: &kept, agents: &agents, scroll: &scroll, now_ms: NOW_MS, defaults };

    assert!(drawing(Some(&chosen)).is_default(&added("a2", "Personal")));
    assert!(!drawing(Some(&chosen)).is_default(&added("a1", "Work")));
    assert!(!drawing(None).is_default(&added("a2", "Personal")), "a hub that keeps no defaults");
}

#[test]
fn found_and_added_are_apart_each_by_what_its_row_calls_it_with_those_disconnected_last() {
    let found = Account { email: Some("bea@example.com".to_string()), ..account("f1") };
    let gone = Account { held_login: None, ..added("a3", "Archive") };
    let accounts = [added("a1", "work"), found.clone(), gone, added("a2", "Contractor"), account("f2")];

    let (found, added) = listed(accounts.iter());

    let ids = |list: Vec<&Account>| list.iter().map(|account| account.id.to_string()).collect::<Vec<_>>();
    assert_eq!(ids(found), ["f1", "f2"], "bea@example.com, then the one called by its ID");
    assert_eq!(ids(added), ["a2", "a1", "a3"], "in any case, and Archive, disconnected, after them");
}

#[gpui_kit::test]
fn disconnecting_with_no_hub_to_ask_says_so_under_that_accounts_row_alone(cx: &mut TestAppContext) {
    let kept = cx.new(|_| Kept::default());

    kept.update(cx, |kept, cx| kept.disconnect(AccountId::from("a"), cx));

    kept.read_with(cx, |kept, _| {
        assert_eq!(kept.not_disconnected(&AccountId::from("a")), Some(words::NO_HUB_TO_DISCONNECT));
        assert_eq!(kept.not_disconnected(&AccountId::from("b")), None);
    });
}

#[test]
fn the_rows_are_drawn_again_as_what_they_say_of_a_time_changes() {
    let midnight_ms = Some(NOW_MS + 9 * 60 * MINUTE_MS);
    let changes = |accounts: &[&Account]| changes_at_ms(accounts.iter().copied(), NOW_MS, midnight_ms);
    let held = |ends_at_ms| Account {
        held_login: Some(HeldLogin { connected_at_ms: 0, ends_at_ms: Some(ends_at_ms) }),
        ..added("b", "Work")
    };

    assert_eq!(changes(&[&account("a")]), None, "a row that says nothing of a time");
    let used = Account { used_at_ms: Some(NOW_MS - 125 * MINUTE_MS), ..account("a") };
    assert_eq!(changes(&[&used]), midnight_ms, "as its card's today becomes yesterday");
    assert_eq!(
        changes(&[&held(NOW_MS + 30 * DAY_MS + 3 * 60 * MINUTE_MS)]),
        Some(NOW_MS + 3 * 60 * MINUTE_MS),
        "as it warns"
    );
    let ends_at_ms = NOW_MS + 3 * 60 * MINUTE_MS;
    let lasting_a_day = Account {
        held_login: Some(HeldLogin { connected_at_ms: ends_at_ms - DAY_MS, ends_at_ms: Some(ends_at_ms) }),
        ..added("d", "Day")
    };
    assert_eq!(
        changes(&[&lasting_a_day]),
        Some(NOW_MS + 60 * MINUTE_MS),
        "as one that lasts a day warns, 2 hours before"
    );
    assert_eq!(changes(&[&held(NOW_MS + 10 * MINUTE_MS)]), Some(NOW_MS + 10 * MINUTE_MS), "as it ends");
    assert_eq!(changes(&[&held(NOW_MS + 20 * DAY_MS)]), midnight_ms, "as the days it has left change");
    assert_eq!(changes(&[&held(NOW_MS - 1), &account("c")]), None, "once it has ended, nothing changes");
    let limited = Account {
        limits: vec![
            limit(LimitWindow::FiveHours, 40, NOW_MS - 1),
            limit(LimitWindow::FiveHours, 40, NOW_MS + 5 * MINUTE_MS),
        ],
        ..account("d")
    };
    assert_eq!(changes(&[&limited]), Some(NOW_MS + 5 * MINUTE_MS), "as a limit shown starts again");
    let weeks = Account { limits: vec![limit(LimitWindow::SevenDays, 40, NOW_MS + 3 * DAY_MS)], ..account("e") };
    assert_eq!(changes(&[&weeks]), midnight_ms, "as the day its card says it starts again on is named otherwise");
}
