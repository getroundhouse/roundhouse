//! What an account's row says: how its login was made, then what else, in each state its login can be in, the default's
//! badge, the order the lists are in, what's said under a row, and when the rows are drawn again.

use gpui_kit::{AppContext as _, ScrollHandle, TestAppContext};
use protocol::fixtures::{account, claude_default, observed};
use protocol::model::{Account, AccountId, AccountIdentity, HeldLogin, LimitWindow, Observation};

use super::super::super::account_row::Tone;
use super::super::kept::Kept;
use super::super::row::changes_at_ms;
use super::super::words::{self, Says};
use super::super::{Drawing, listed};
use super::made_up::{DAY_MS, MINUTE_MS, NOW_MS, added, claude, holding, limit};
use client_core::Agents;

/// What the row of `account` says after how its login was made, with what `observed` says of it, at [`NOW_MS`], with
/// days in UTC.
fn says(cx: &mut TestAppContext, account: &Account, observed: &[Observation]) -> Says {
    cx.update(|cx| {
        look::hold_still(NOW_MS, cx);
        let standing = account.login_agent.map(|_| words::standing(account.held_login.as_ref(), NOW_MS, cx));
        words::says(&holding([account], observed.iter().cloned()), account, standing.as_ref(), NOW_MS)
    })
}

/// How `account`'s login was made, as its row says, with what `observed` says of it, by how the hub describes
/// `agents`.
fn how(account: &Account, observed: &[Observation], agents: &Agents) -> Option<String> {
    words::how(&holding([account], observed.iter().cloned()), account, agents)
}

/// A login the hub holds that lasts a year and ends at `ends_at_ms`.
fn ending(ends_at_ms: i64) -> HeldLogin {
    HeldLogin { connected_at_ms: ends_at_ms - 365 * DAY_MS, ends_at_ms: Some(ends_at_ms) }
}

/// Two limits heard of `account`.
fn limited(account: &Account) -> Observation {
    let limits = vec![
        limit(LimitWindow::FiveHours, 29, NOW_MS + 60 * MINUTE_MS),
        limit(LimitWindow::SevenDays, 4, NOW_MS + DAY_MS),
    ];
    observed::limits(account.id.clone(), limits)
}

/// What an agent under the account `id` said of it: its plan, and how it's logged in, if `login`.
fn plan(id: &str, login: Option<&str>) -> Observation {
    let said =
        AccountIdentity { plan: Some("Max".to_string()), login: login.map(str::to_string), ..Default::default() };
    observed::identity(id, said)
}

#[test]
fn how_a_login_was_made_is_its_kind_for_one_added_then_its_plan_and_how_its_logged_in() {
    let agents = Agents::new(vec![claude(None)]);
    let found = [plan("f", Some("Claude account"))];

    assert_eq!(how(&account("f"), &found, &agents).as_deref(), Some("Max · Claude account"));
    assert_eq!(how(&added("a", "Work"), &[], &agents).as_deref(), Some("Claude Code"));
    assert_eq!(how(&added("a", "Work"), &[plan("a", None)], &agents).as_deref(), Some("Claude Code · Max"));
    assert_eq!(how(&account("f"), &[], &agents), None, "an account nothing has said of");
    assert_eq!(how(&added("a", "Work"), &[], &Agents::default()), None, "before the hub has described Claude");
}

#[gpui_kit::test]
fn a_login_that_needs_seeing_to_is_said_before_its_limits(cx: &mut TestAppContext) {
    let soon = Account { held_login: Some(ending(NOW_MS + 9 * DAY_MS)), ..added("a", "Personal") };
    let ended = Account { held_login: Some(ending(NOW_MS - 3 * DAY_MS)), ..added("a", "Side project") };
    let gone = Account { held_login: None, ..added("a", "Contractor") };
    let says = |cx: &mut TestAppContext, account: &Account| says(cx, account, &[limited(account)]);

    assert_eq!(says(cx, &soon), Says::Standing("Ends in 9 days, on Nov 10, 2026".to_string(), Tone::Warning));
    assert_eq!(says(cx, &ended), Says::Standing("Ended on Oct 29, 2026".to_string(), Tone::Problem));
    assert_eq!(says(cx, &gone), Says::Standing("Disconnected".to_string(), Tone::Plain));
}

#[gpui_kit::test]
fn otherwise_a_row_says_whats_left_of_its_limits_or_that_none_has_been_heard(cx: &mut TestAppContext) {
    let far_off = Account { held_login: Some(ending(NOW_MS + 300 * DAY_MS)), ..added("a", "Work") };
    let found = account("f");

    let left = Says::Limits(vec![("5h".into(), 71, None), ("7d".into(), 96, None)]);
    assert_eq!(says(cx, &found, &[limited(&found)]), left, "a login found where agents ran");
    assert_eq!(says(cx, &far_off, &[limited(&far_off)]), left, "and one held that ends after it warns");
    assert_eq!(says(cx, &far_off, &[]), Says::NoLimitsYet);
    let started_again = observed::limits("a", vec![limit(LimitWindow::FiveHours, 29, NOW_MS)]);
    assert_eq!(says(cx, &far_off, &[started_again]), Says::Nothing, "every limit heard has started again");
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
    let chosen = [claude_default(&"a2".into())];
    let records = holding([], []);
    let drawing = |defaults| Drawing {
        records: &records,
        kept: &kept,
        agents: &agents,
        scroll: &scroll,
        now_ms: NOW_MS,
        defaults,
    };

    assert!(drawing(Some(&chosen)).is_default(&added("a2", "Personal")));
    assert!(!drawing(Some(&chosen)).is_default(&added("a1", "Work")));
    assert!(!drawing(None).is_default(&added("a2", "Personal")), "a hub that keeps no defaults");
}

#[test]
fn found_and_added_are_apart_each_by_what_its_row_calls_it_with_those_disconnected_last() {
    let gone = Account { held_login: None, ..added("a3", "Archive") };
    let accounts = [added("a1", "work"), account("f1"), gone, added("a2", "Contractor"), account("f2")];
    let bea = AccountIdentity { email: Some("bea@example.com".to_string()), ..AccountIdentity::default() };

    let records = holding(&accounts, [observed::identity("f1", bea)]);
    let (found, added) = listed(&records);

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
    let changes_with = |accounts: &[&Account], observed: &[Observation]| {
        let records = holding(accounts.iter().copied(), observed.iter().cloned());
        changes_at_ms(&records, accounts.iter().copied(), NOW_MS, midnight_ms)
    };
    let changes = |accounts: &[&Account]| changes_with(accounts, &[]);
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
    let limited = observed::limits(
        "d",
        vec![limit(LimitWindow::FiveHours, 40, NOW_MS - 1), limit(LimitWindow::FiveHours, 40, NOW_MS + 5 * MINUTE_MS)],
    );
    assert_eq!(
        changes_with(&[&account("d")], &[limited]),
        Some(NOW_MS + 5 * MINUTE_MS),
        "as a limit shown starts again"
    );
    let weeks = observed::limits("e", vec![limit(LimitWindow::SevenDays, 40, NOW_MS + 3 * DAY_MS)]);
    assert_eq!(
        changes_with(&[&account("e")], &[weeks]),
        midnight_ms,
        "as the day its card says it starts again on is named otherwise"
    );
}
