use std::path::Path;

use protocol::model::{AccountSource, AgentAccount, AgentKind, CacheTtl, PromptCache};

use super::launch_rows::Recorded;
use super::*;
use crate::login_files::LoginFile;
use crate::store::tests::{idle_session, store_with_session};
use crate::store::{Connecting, SessionLogin, now_ms};

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

/// A run given no login stops a chat on none naming the account of a login the hub holds, as one from before chats were
/// put on accounts can, since its agent isn't under it. A chat on the hub's login goes on naming the account it's on,
/// which the launch that put it there named, and only a move changes.
#[test]
fn a_run_given_no_login_stops_a_chat_on_none_naming_a_held_logins_account_but_not_a_chat_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let s = SessionId::from("s");
    lock(&store.writer).execute("UPDATE sessions SET account_id = 'given' WHERE id = 's'", []).unwrap();

    let on_none = store.file_run_login(&s, RunId::new(1)).unwrap();
    let named_on_none = account_of_s(&store);
    store.record_run_login(&s, RunId::new(2), &"given".into()).unwrap();
    let filed = store.file_run_login(&s, RunId::new(2)).unwrap();
    let on_it = store.file_run_login(&s, RunId::new(3)).unwrap();

    assert_eq!(on_none, RunLogin { account: None, session_changed: true });
    assert_eq!(named_on_none, None);
    let launch_named_it = RunLogin { account: Some("given".into()), session_changed: false };
    assert_eq!(filed, launch_named_it);
    assert_eq!(on_it, RunLogin { account: None, session_changed: false });
    assert_eq!(account_of_s(&store), Some("given".into()));
}

#[test]
fn a_launch_given_a_login_after_the_person_sent_its_agent_something_is_when_the_logins_account_was_last_used() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let s = SessionId::from("s");
    let sent = store.append(&s, None, &crate::store::events::tests::prompt("go", 0), None, None, None).unwrap();

    let recorded = store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();

    assert_eq!(recorded.account_changed.and_then(|account| account.used_at_ms), Some(sent.event.ts_ms));
}

#[test]
fn a_run_given_a_login_its_account_was_used_after_in_another_chat_leaves_that_later_use() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let s = SessionId::from("s");
    store.append(&s, None, &crate::store::events::tests::prompt("go", 0), None, None, None).unwrap();
    lock(&store.writer).execute("UPDATE accounts SET used_at_ms = ?", [i64::MAX]).unwrap();

    let recorded = store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();

    assert_eq!(recorded.account_changed, None);
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

    assert_eq!(filed, RunLogin { account: None, session_changed: false });
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

/// A chat's first launch under a login the hub holds puts it on the hub's login for that account, which its record
/// names from then, and which moves its record on for every client to hear. A later launch under another login leaves
/// it there.
#[test]
fn a_chats_first_launch_under_a_held_login_puts_it_on_the_hubs_for_that_account_and_a_later_one_leaves_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let file = LoginFile::new();
    store.connect_login(&connecting(&"other".into(), "Personal", &file)).unwrap();
    let s = SessionId::from("s");
    let before = store.get_session(&s).unwrap().session;

    let first = store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();
    let after = store.get_session(&s).unwrap().session;
    let later = store.record_run_login(&s, RunId::new(2), &"other".into()).unwrap();

    let on_given = SessionLogin::Hub("given".into());
    assert_eq!((before.account_source, before.account), (None, None));
    assert_eq!((&first.on, first.session_changed, first.forgot_cache), (&on_given, true, false));
    assert_eq!((after.account_source, after.account), (Some(AccountSource::Hub), Some("given".into())));
    assert!(after.rev > before.rev);
    let left = Recorded { on: on_given, session_changed: false, forgot_cache: false, account_changed: None };
    assert_eq!(later, left);
    assert_eq!(store.get_session(&s).unwrap().session.rev, after.rev, "nothing about it changed");
}

/// A chat's first launch under a held login forgets what was cached when its record named another account, or none, as
/// a move does, so neither a retry after that launch failed nor a later launch takes the first request under the
/// account for one that found the cache expired. One whose record named the account already keeps what was cached.
#[test]
fn a_chats_first_launch_under_a_held_login_forgets_another_accounts_cache_as_a_move_does() {
    let cache = PromptCache { used_at_ms: 5, ttl: CacheTtl::OneHour, model: None, tokens: 9 };
    let launched = |named: Option<&str>| {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_a_login(dir.path());
        let file = LoginFile::new();
        store.connect_login(&connecting(&"other".into(), "Personal", &file)).unwrap();
        let s = SessionId::from("s");
        lock(&store.writer).execute("UPDATE sessions SET account_id = ? WHERE id = 's'", [named]).unwrap();
        store.set_session_cache(&s, Some(&cache)).unwrap();
        let recorded = store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();
        (recorded.forgot_cache, store.get_session(&s).unwrap().session.prompt_cache)
    };

    assert_eq!(launched(Some("other")), (true, None), "another account's cache may not hold the conversation");
    assert_eq!(launched(None), (true, None), "a record that named no account can't say whose cache it was");
    assert_eq!(launched(Some("given")), (false, Some(cache.clone())));
}

/// What a run under another login than the one a chat on the hub's is on says it's under leaves the chat on its
/// account, as when its agent from before a move is still running: filing the login the hub gave the run, and the
/// account its worker says an ambient one is, or that it can't tell.
#[test]
fn filing_a_run_under_another_login_leaves_a_chat_on_the_hubs_on_its_account() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let file = LoginFile::new();
    store.connect_login(&connecting(&"other".into(), "Personal", &file)).unwrap();
    let s = SessionId::from("s");
    store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();
    store.record_run_login(&s, RunId::new(2), &"other".into()).unwrap();
    let before = store.get_session(&s).unwrap().session;
    let ambient = AgentAccount { id: Some("claude-0123456789abcdef".into()), logged_in: true, ..Default::default() };

    let filed = store.file_run_login(&s, RunId::new(2)).unwrap();
    let worker_said = store.launched_under(&s, &ambient).unwrap();
    let worker_couldnt_tell = store.launched_under(&s, &AgentAccount::default()).unwrap();

    assert_eq!(filed, RunLogin { account: Some("other".into()), session_changed: false });
    assert!(!worker_said.session && !worker_couldnt_tell.session);
    let after = store.get_session(&s).unwrap().session;
    assert_eq!((SessionLogin::of_record(&after), after.rev), (SessionLogin::Hub("given".into()), before.rev));
}

/// A chat whose agent ran under an ambient login is put on it once its worker says whose that is, and stays there
/// whatever its later launches are given, until it's moved. One whose worker can't say, as with nobody logged in, is
/// left on none, so its next launch given a login puts it on the hub's.
#[test]
fn a_chat_is_put_on_the_ambient_login_once_its_worker_says_whose_it_is_until_its_moved() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_a_login(dir.path());
    let mut other = idle_session(now_ms());
    (other.session.id, other.agent_session_id) = ("t".into(), Some("t".into()));
    store.insert_session(&other, None).unwrap();
    let (s, t) = (SessionId::from("s"), SessionId::from("t"));
    let before = store.get_session(&s).unwrap().session;
    let ambient = AgentAccount { id: Some("claude-0123456789abcdef".into()), logged_in: true, ..Default::default() };

    let put = store.launched_under(&s, &ambient).unwrap();
    let after = store.get_session(&s).unwrap().session;
    let again = store.launched_under(&s, &ambient).unwrap();
    let given = store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();
    let still = store.get_session(&s).unwrap().session;
    store.move_session(&s, &"given".into()).unwrap();
    let moved = store.get_session(&s).unwrap().session;
    let nobody = store.launched_under(&t, &AgentAccount::default()).unwrap();
    let unnamed = store.get_session(&t).unwrap().session;
    let given_after_nobody = store.record_run_login(&t, RunId::new(2), &"given".into()).unwrap();

    assert!(put.ambient && put.session && !again.ambient && !again.session);
    assert_eq!((after.account_source, &after.account), (Some(AccountSource::Ambient), &ambient.id));
    assert_eq!(SessionLogin::of_record(&after), SessionLogin::Ambient);
    assert!(after.rev > before.rev);
    assert_eq!((given.on, given.session_changed), (SessionLogin::Ambient, false));
    assert_eq!(still.rev, after.rev, "nothing about it changed");
    assert_eq!(SessionLogin::of_record(&moved), SessionLogin::Hub("given".into()));
    assert!(!nobody.ambient);
    assert_eq!(SessionLogin::of_record(&unnamed), SessionLogin::NoneYet);
    assert_eq!(given_after_nobody.on, SessionLogin::Hub("given".into()));
}

/// A record that says the hub's login but names no account is on none, so its next launch given a login fills it in.
/// One given none leaves it so, rather than put it on the ambient login, since it was put on the hub's.
#[test]
fn a_chat_whose_record_says_the_hubs_but_names_no_account_is_put_on_its_next_launchs() {
    let (dir, other_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (store, other) = (store_with_a_login(dir.path()), store_with_session(other_dir.path()));
    let s = SessionId::from("s");
    for store in [&store, &other] {
        lock(&store.writer).execute("UPDATE sessions SET account_source = 'hub' WHERE id = 's'", []).unwrap();
        assert_eq!(SessionLogin::of_record(&store.get_session(&s).unwrap().session), SessionLogin::NoneYet);
    }

    let ambient = AgentAccount { id: Some("claude-0123456789abcdef".into()), logged_in: true, ..Default::default() };

    let given = store.record_run_login(&s, RunId::new(1), &"given".into()).unwrap();
    let given_none = other.launched_under(&s, &ambient).unwrap();

    assert_eq!((given.on, account_of_s(&store)), (SessionLogin::Hub("given".into()), Some("given".into())));
    assert!(!given_none.ambient && !given_none.session);
    assert_eq!(SessionLogin::of_record(&other.get_session(&s).unwrap().session), SessionLogin::NoneYet);
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
    assert_eq!((after.account_source, after.account, after.prompt_cache), (Some(AccountSource::Hub), other, None));
    assert!(fresh_moved.forgot_cache, "a record that named no account can't say whose cache it was");
}
