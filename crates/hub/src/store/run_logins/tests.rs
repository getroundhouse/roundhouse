use std::path::Path;

use protocol::model::{AgentAccount, AgentKind};

use super::*;
use crate::login_files::LoginFile;
use crate::store::tests::{idle_session, store_with_session};
use crate::store::{Connecting, now_ms};

/// The account the session `s` names.
fn account_of_s(store: &Store) -> Option<AccountId> {
    store.get_session(&"s".into()).unwrap().session.account
}

/// Every run the hub's record says it gave a login, in any chat, by chat and account.
fn runs_given_logins(store: &Store) -> Vec<(String, u64, String)> {
    let conn = lock(&store.reader);
    let mut statement =
        conn.prepare("SELECT session_id, run, account_id FROM runs_under_logins ORDER BY session_id, run").unwrap();
    let rows =
        statement.query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)?.cast_unsigned(), row.get(2)?))).unwrap();
    rows.collect::<rusqlite::Result<_>>().unwrap()
}

/// A store with the chat `s`, and an account whose ID the hub chose, `given`, for its runs to be given the login of.
fn store_with_a_login(dir: &Path) -> Store {
    let store = store_with_session(dir);
    let file = LoginFile::new();
    let connecting = Connecting {
        account: &"given".into(),
        agent: AgentKind::Claude,
        name: "Work",
        file: &file,
        connected_at_ms: 100,
        ends_at_ms: None,
    };
    store.connect_login(&connecting).unwrap();
    store
}

#[test]
fn a_run_given_a_login_is_filed_under_its_account_and_one_given_none_stops_the_chat_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let s = SessionId::from("s");
    store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();

    let filed = store.file_run_login(&s, RunId::new(1)).unwrap();
    let again = store.file_run_login(&s, RunId::new(1)).unwrap();
    let named = account_of_s(&store);
    let given_none = store.file_run_login(&s, RunId::new(2)).unwrap();

    assert_eq!(filed, RunLogin { account: Some("given".into()), session_changed: true, account_changed: None });
    assert_eq!(again, RunLogin { account: Some("given".into()), session_changed: false, account_changed: None });
    assert_eq!(named, Some("given".into()));
    assert_eq!(given_none, RunLogin { account: None, session_changed: true, account_changed: None });
    assert_eq!(account_of_s(&store), None);
}

#[test]
fn a_run_given_a_login_after_the_person_sent_its_agent_something_is_when_the_logins_account_was_last_used() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let s = SessionId::from("s");
    let sent = store.append(&s, None, &crate::store::events::tests::prompt("go", 0), None, None, None).unwrap();
    store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();

    let filed = store.file_run_login(&s, RunId::new(1)).unwrap();
    let again = store.file_run_login(&s, RunId::new(1)).unwrap();

    assert_eq!(filed.account_changed.and_then(|account| account.used_at_ms), Some(sent.event.ts_ms));
    assert_eq!(again.account_changed, None, "filing the same run again changes nothing");
}

#[test]
fn a_run_given_a_login_its_account_was_used_after_in_another_chat_leaves_that_later_use() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let s = SessionId::from("s");
    store.append(&s, None, &crate::store::events::tests::prompt("go", 0), None, None, None).unwrap();
    lock(&store.writer).execute("UPDATE accounts SET used_at_ms = ?", [i64::MAX]).unwrap();
    store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();

    let filed = store.file_run_login(&s, RunId::new(1)).unwrap();

    assert_eq!(filed.account_changed, None);
    assert_eq!(store.account(&"given".into()).unwrap().unwrap().used_at_ms, Some(i64::MAX));
}

#[test]
fn a_run_given_no_login_leaves_the_chat_naming_the_account_its_worker_said() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let worker_said =
        AgentAccount { id: Some("claude-0123456789abcdef".into()), logged_in: true, ..Default::default() };
    store.launched_under(&"s".into(), &worker_said).unwrap();

    let filed = store.file_run_login(&"s".into(), RunId::new(1)).unwrap();

    assert_eq!(filed, RunLogin { account: None, session_changed: false, account_changed: None });
    assert_eq!(account_of_s(&store), worker_said.id);
}

#[test]
fn once_a_worker_says_which_run_is_going_the_logins_the_chats_other_runs_were_given_are_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let mut other = idle_session(now_ms());
    (other.session.id, other.agent_session_id) = ("t".into(), Some("t".into()));
    store.insert_session(&other, None).unwrap();
    for run in 1..=3 {
        store.record_run_login(&"s".into(), RunId::new(run), &"given".into()).unwrap();
    }
    store.record_run_login(&"t".into(), RunId::new(4), &"given".into()).unwrap();

    store.forget_other_run_logins(&"s".into(), RunId::new(2)).unwrap();

    let given = |chat: &str, run| (chat.to_string(), run, "given".to_string());
    assert_eq!(runs_given_logins(&store), [given("s", 2), given("t", 4)], "another chat's are its own");
}

#[test]
fn the_logins_a_chats_runs_were_given_go_with_the_chat() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    store.record_run_login(&"s".into(), RunId::new(1), &"given".into()).unwrap();

    let deleted = store.delete_unused_session(&"s".into()).unwrap();

    assert!(deleted.is_some());
    assert_eq!(runs_given_logins(&store), []);
}
