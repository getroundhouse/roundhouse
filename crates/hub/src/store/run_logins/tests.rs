use std::path::Path;

use protocol::model::{AgentAccount, AgentKind, CacheTtl, PromptCache};

use super::launch_rows::Recorded;
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

/// Connecting the login in `file` to `account`, an account whose ID the hub chose, called `name`.
fn connecting<'a>(account: &'a AccountId, name: &'a str, file: &'a LoginFile) -> Connecting<'a> {
    Connecting { account, agent: AgentKind::Claude, name, file, connected_at_ms: 100, ends_at_ms: None }
}

/// A store with the chat `s`, and an account whose ID the hub chose, `given`, for its runs to be given the login of.
fn store_with_a_login(dir: &Path) -> Store {
    let store = store_with_session(dir);
    let file = LoginFile::new();
    store.connect_login(&connecting(&"given".into(), "Work", &file)).unwrap();
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

/// A chat's first launch under a login the hub holds puts it on that login's account, which moves its record on for
/// every client to hear, and a later launch under another login leaves it there.
#[test]
fn a_chats_first_launch_under_a_held_login_puts_it_on_that_account_and_a_later_one_leaves_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let file = LoginFile::new();
    store.connect_login(&connecting(&"other".into(), "Personal", &file)).unwrap();
    let s = SessionId::from("s");
    let before = store.get_session(&s).unwrap().session;

    let first = store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();
    let after = store.get_session(&s).unwrap().session;
    let later = store.record_run_login(&s, RunId::new(2), &"other".into()).unwrap();

    assert_eq!(before.assigned_account, None);
    assert_eq!(first, Recorded { before: None, on: Some("given".into()) });
    assert_eq!(after.assigned_account, Some("given".into()));
    assert!(after.rev > before.rev);
    assert_eq!(later, Recorded { before: None, on: Some("given".into()) });
    assert_eq!(store.get_session(&s).unwrap().session.rev, after.rev, "nothing about it changed");
}

/// Moving a chat puts it on the account, has its record name it, and forgets what was cached when the record named
/// another account, or none, since that account's cache may not hold the conversation. A move to where it is changes
/// nothing.
#[test]
fn a_moved_chat_is_on_its_account_and_names_it_and_forgets_another_accounts_cache() {
    let (dir, fresh_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (store, fresh) = (store_with_a_login(dir.path()), store_with_a_login(fresh_dir.path()));
    let file = LoginFile::new();
    store.connect_login(&connecting(&"other".into(), "Personal", &file)).unwrap();
    let (s, cache) =
        (SessionId::from("s"), PromptCache { used_at_ms: 5, ttl: CacheTtl::OneHour, model: None, tokens: 9 });
    store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();
    store.file_run_login(&s, RunId::new(1)).unwrap();
    store.set_session_cache(&s, Some(&cache)).unwrap();
    fresh.set_session_cache(&s, Some(&cache)).unwrap();
    let before = store.get_session(&s).unwrap().session;

    let in_place = store.move_session(&s, &"given".into()).unwrap();
    let kept = store.get_session(&s).unwrap().session;
    let moved = store.move_session(&s, &"other".into()).unwrap();
    let after = store.get_session(&s).unwrap().session;
    let fresh_moved = fresh.move_session(&s, &"given".into()).unwrap();

    assert!(!in_place.forgot_cache);
    assert_eq!((kept.rev, kept.prompt_cache), (before.rev, Some(cache)), "nothing about it changed");
    assert!(moved.forgot_cache);
    let other = Some(AccountId::from("other"));
    assert_eq!((after.assigned_account, after.account, after.prompt_cache), (other.clone(), other, None));
    assert!(fresh_moved.forgot_cache, "a record that named no account can't say whose cache it was");
}
