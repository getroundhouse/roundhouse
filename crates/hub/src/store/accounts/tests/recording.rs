use super::super::*;
use crate::store::tests::store_with_session;
use protocol::model::Rev;

/// Logged in as `email`, under an account named for it.
pub(super) fn logged_in(email: &str, plan: &str) -> AgentAccount {
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
pub(super) fn named(store: &Store) -> Option<AccountId> {
    store.get_session(&"s".into()).unwrap().session.account
}

/// The record says nothing of who the account is, which the hub keeps as an observation, as
/// `crate::hub::observations::AccountIdentity` says.
#[test]
fn a_launch_names_its_account_on_the_session_and_makes_its_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session.rev;

    let launched = store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();

    let account = launched.account.expect("a new record");
    assert_eq!(account.id.as_str(), "claude-ada@example.com");
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
    store.account_made(&"grace".into()).unwrap();

    let records = store.list_records().unwrap();

    let listed: Vec<&str> = records.accounts.iter().map(|account| account.id.as_str()).collect();
    assert_eq!(listed, ["claude-ada@example.com", "grace"]);
    assert!(records.accounts.iter().all(|account| account.rev > Rev::default() && account.rev <= records.rev));
}
