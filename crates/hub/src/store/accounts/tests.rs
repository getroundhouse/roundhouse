use protocol::model::Rev;

use super::*;
use crate::store::tests::{open_in, store_with_session};

fn limit(window: LimitWindow, used_percent: u8) -> UsageLimit {
    UsageLimit { window, used_percent, resets_at_ms: Some(9_000), heard_at_ms: None }
}

/// What's kept for the account, as window, how much is used, and when that was heard.
fn kept(store: &Store, account: &str) -> Vec<(LimitWindow, u8, Option<i64>)> {
    let limits = store.account_limits(&account.into()).unwrap();
    limits.iter().map(|limit| (limit.window, limit.used_percent, limit.heard_at_ms)).collect()
}

/// Logged in as `email`, under an account named for it.
fn logged_in(email: &str, plan: &str) -> AgentAccount {
    AgentAccount {
        id: Some(format!("claude-{email}").into()),
        logged_in: true,
        model_provider: Some("Anthropic API".to_string()),
        plan: Some(plan.to_string()),
        login: Some("Claude account".to_string()),
        email: Some(email.to_string()),
    }
}

/// The account the session `s` names.
fn named(store: &Store) -> Option<AccountId> {
    store.get_session(&"s".into()).unwrap().session.account
}

#[test]
fn each_accounts_limits_are_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());

    store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap();
    store.hear_limits(&"grace".into(), &[limit(LimitWindow::SevenDays, 80)], 200).unwrap();

    assert_eq!(kept(&store, "ada"), [(LimitWindow::SevenDays, 4, Some(100))]);
    assert_eq!(kept(&store, "grace"), [(LimitWindow::SevenDays, 80, Some(200))]);
    assert_eq!(kept(&store, "nobody"), []);
}

#[test]
fn hearing_of_one_window_leaves_the_other_and_the_shortest_comes_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let both = [limit(LimitWindow::SevenDays, 4), limit(LimitWindow::FiveHours, 60)];
    store.hear_limits(&"ada".into(), &both, 100).unwrap();

    store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 5)], 200).unwrap();

    assert_eq!(kept(&store, "ada"), [(LimitWindow::FiveHours, 60, Some(100)), (LimitWindow::SevenDays, 5, Some(200))]);
}

#[test]
fn what_was_heard_earlier_never_replaces_what_was_heard_later_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let later = store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 9)], 200).unwrap().unwrap();

    let earlier = store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap();

    assert_eq!(kept(&store, "ada"), [(LimitWindow::SevenDays, 9, Some(200))]);
    assert_eq!(earlier, None, "so the feed isn't told");
    assert_eq!(store.account(&"ada".into()).unwrap().unwrap().rev, later.rev);
}

#[test]
fn a_window_with_no_name_here_isnt_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());

    store.hear_limits(&"ada".into(), &[limit(LimitWindow::Unknown, 4)], 100).unwrap();

    assert_eq!(kept(&store, "ada"), []);
}

#[test]
fn limits_are_still_there_when_the_database_is_opened_again() {
    let dir = tempfile::tempdir().unwrap();
    store_and_drop(dir.path());

    assert_eq!(kept(&open_in(dir.path()), "ada"), [(LimitWindow::SevenDays, 4, Some(100))]);
}

fn store_and_drop(dir: &std::path::Path) {
    open_in(dir).hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap();
}

#[test]
fn hearing_of_a_change_to_a_limit_gives_the_accounts_record_a_newer_revision_with_it_in() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let first = store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap().unwrap();

    let changed = store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 5)], 200).unwrap().unwrap();

    assert!(changed.rev > first.rev);
    assert_eq!(changed.limits, [UsageLimit { heard_at_ms: None, ..limit(LimitWindow::SevenDays, 5) }]);
    assert_eq!(store.list_records().unwrap().accounts, [changed]);
}

#[test]
fn hearing_the_same_again_isnt_a_change_to_the_record_but_is_kept_as_heard_later() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let first = store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 100).unwrap().unwrap();

    let again = store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 4)], 200).unwrap();

    assert_eq!(again, None, "so the feed isn't told what it knows");
    assert_eq!(store.account(&"ada".into()).unwrap().unwrap().rev, first.rev);
    // So something heard between the two, and only said now, doesn't replace it.
    assert_eq!(kept(&store, "ada"), [(LimitWindow::SevenDays, 4, Some(200))]);
    store.hear_limits(&"ada".into(), &[limit(LimitWindow::SevenDays, 3)], 150).unwrap();
    assert_eq!(kept(&store, "ada"), [(LimitWindow::SevenDays, 4, Some(200))]);
}

#[test]
fn a_launch_names_its_account_on_the_session_and_says_who_it_is_on_the_account() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session.rev;

    let launched = store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();

    let account = launched.account.expect("a new record");
    assert_eq!((account.email.as_deref(), account.plan.as_deref()), (Some("ada@example.com"), Some("Max")));
    assert!(launched.session);
    assert_eq!(named(&store), Some(account.id.clone()));
    assert!(store.get_session(&"s".into()).unwrap().session.rev > before, "the feed has to announce it");
}

#[test]
fn a_launch_under_the_same_account_as_before_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();
    let rev = store.list_records().unwrap().rev;

    let launched = store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();

    assert_eq!(launched, Launched::default());
    assert_eq!(store.list_records().unwrap().rev, rev);
}

#[test]
fn a_new_plan_is_a_change_to_the_account_and_not_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.launched_under(&"s".into(), &logged_in("ada@example.com", "Pro")).unwrap();

    let launched = store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();

    assert_eq!(launched.account.and_then(|account| account.plan).as_deref(), Some("Max"));
    assert!(!launched.session);
}

#[test]
fn whose_account_it_is_stays_beside_the_limits_heard_before() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let under = logged_in("ada@example.com", "Max");
    let id = under.id.clone().unwrap();
    store.hear_limits(&id, &[limit(LimitWindow::SevenDays, 4)], 100).unwrap();

    let account = store.launched_under(&"s".into(), &under).unwrap().account.unwrap();

    assert_eq!(account.limits.len(), 1, "who it is joins what was heard");
    assert_eq!(account.email.as_deref(), Some("ada@example.com"));
}

#[test]
fn nobody_logged_in_leaves_the_session_naming_no_account() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();
    let nobody = AgentAccount { id: None, logged_in: false, ..AgentAccount::default() };

    let launched = store.launched_under(&"s".into(), &nobody).unwrap();

    assert_eq!((launched.account, launched.session), (None, true));
    assert_eq!(named(&store), None);
    assert_eq!(store.list_records().unwrap().accounts.len(), 1, "the account's record stays");
}

#[test]
fn every_account_is_listed_with_every_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();
    store.hear_limits(&"grace".into(), &[limit(LimitWindow::SevenDays, 80)], 100).unwrap();

    let records = store.list_records().unwrap();

    let listed: Vec<(&str, Option<&str>)> =
        records.accounts.iter().map(|account| (account.id.as_str(), account.email.as_deref())).collect();
    assert_eq!(listed, [("claude-ada@example.com", Some("ada@example.com")), ("grace", None)]);
    assert!(records.accounts.iter().all(|account| account.rev > Rev::default() && account.rev <= records.rev));
}

#[test]
fn limits_kept_before_accounts_were_records_become_accounts_with_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "
        INSERT INTO account_limits (account_id, window, used_percent, resets_at_ms, heard_at_ms)
            VALUES ('ada', 'seven_days', 4, 9000, 100), ('ada', 'five_hours', 60, 800, 150);
    ";

    let store = crate::store::older::store_from_before(dir.path(), crate::store::schema::BEFORE_ACCOUNTS, rows);

    let accounts = store.list_records().unwrap().accounts;
    assert_eq!(accounts.len(), 1);
    assert_eq!((accounts[0].id.as_str(), accounts[0].email.as_deref()), ("ada", None), "who it is comes at a launch");
    assert_eq!(accounts[0].limits.len(), 2);
    assert!(accounts[0].rev > Rev::default(), "it has a revision, as every record does");
}
