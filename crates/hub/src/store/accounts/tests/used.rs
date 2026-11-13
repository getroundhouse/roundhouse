//! When each account was last used: what the person sends, and the launch that first names a session's account.

use super::*;

/// When the account was last used, as its record says.
fn used_at(store: &Store, account: &str) -> Option<i64> {
    store.account(&account.into()).unwrap().unwrap().used_at_ms
}

#[test]
fn what_the_person_sends_a_chats_agent_is_when_the_chats_account_was_last_used_and_nothing_else_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();
    let before = store.account(&"claude-ada@example.com".into()).unwrap().unwrap();

    let sent = store.append(&"s".into(), None, &prompt("go", 0), None, None, None).unwrap();
    let after_sending = store.account(&"claude-ada@example.com".into()).unwrap().unwrap();
    // Earlier than any clock could say, so what the agent records next can't land on the same moment and go unseen.
    lock(&store.writer).execute("UPDATE accounts SET used_at_ms = 0", []).unwrap();
    let agents_own = store.append(&"s".into(), Some(RunId::new(1)), &prompt("the agent's own", 0), None, None, None);

    assert_eq!(before.used_at_ms, None, "launching isn't using it");
    assert_eq!(after_sending.used_at_ms, Some(sent.event.ts_ms));
    assert_eq!(sent.account, Some(after_sending.clone()), "the hub is given the record to announce");
    assert!(after_sending.rev > before.rev, "it's a change to the account's record");
    assert_eq!(used_at(&store, "claude-ada@example.com"), Some(0), "what the agent records isn't use");
    assert_eq!(agents_own.unwrap().account, None);
}

#[test]
fn a_chats_account_named_once_the_person_had_sent_its_agent_something_was_used_then() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let sent = store.append(&"s".into(), None, &prompt("go", 0), None, None, None).unwrap();

    let launched = store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();
    let again = store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();

    assert_eq!(launched.account.and_then(|account| account.used_at_ms), Some(sent.event.ts_ms));
    assert_eq!(again, Launched::default(), "filing the same launch again changes nothing");
}

#[test]
fn an_account_used_later_by_another_chat_keeps_that() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();
    lock(&store.writer).execute("UPDATE accounts SET used_at_ms = ?", [i64::MAX]).unwrap();

    store.append(&"s".into(), None, &prompt("go", 0), None, None, None).unwrap();

    assert_eq!(used_at(&store, "claude-ada@example.com"), Some(i64::MAX));
}

/// As when the person sent a chat something while no agent ran, and its next launch ran under another login, or the hub
/// started its agent by itself under one: the account it moves to wasn't sent what the one before was.
#[test]
fn a_chat_that_moves_to_another_account_leaves_what_it_was_sent_with_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();
    let sent = store.append(&"s".into(), None, &prompt("go", 0), None, None, None).unwrap();

    let moved = store.launched_under(&"s".into(), &logged_in("grace@example.com", "Pro")).unwrap();

    assert_eq!(named(&store), Some("claude-grace@example.com".into()));
    assert_eq!(moved.account.map(|account| account.used_at_ms), Some(None), "a new account, not used yet");
    assert_eq!(used_at(&store, "claude-ada@example.com"), Some(sent.event.ts_ms));
}

/// As when another chat under the same account was sent something after this one, whose agent hadn't launched yet.
#[test]
fn a_launch_filed_after_another_chat_used_its_account_leaves_that_later_use() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.hear_limits(&"claude-ada@example.com".into(), &[limit(LimitWindow::SevenDays, 4)], false, 100).unwrap();
    store.append(&"s".into(), None, &prompt("go", 0), None, None, None).unwrap();
    lock(&store.writer).execute("UPDATE accounts SET used_at_ms = ?", [i64::MAX]).unwrap();

    let launched = store.launched_under(&"s".into(), &logged_in("ada@example.com", "Max")).unwrap();

    assert_eq!(used_at(&store, "claude-ada@example.com"), Some(i64::MAX));
    assert_eq!(launched.account.map(|account| account.used_at_ms), Some(Some(i64::MAX)), "who it is changed");
}

#[test]
fn an_account_from_before_was_last_used_when_the_person_last_sent_something_in_a_chat_that_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "
        INSERT INTO accounts (id) VALUES ('ada'), ('grace'), ('unused');
        INSERT INTO sessions (id, workspace_id, agent, model, effort, permission_mode, agent_session_id, status,
            last_seq, created_at_ms, updated_at_ms, user_active_at_ms, account_id)
            VALUES ('s', 'w', 'claude', 'opus', 'high', 'bypassPermissions', 's', 'idle', 0, 100, 9000, 1800, 'ada'),
                ('t', 'w', 'claude', 'opus', 'high', 'bypassPermissions', 't', 'idle', 0, 100, 9000, 2400, 'ada'),
                ('u', 'w', 'claude', 'opus', 'high', 'bypassPermissions', 'u', 'idle', 0, 100, 9000, 600, 'grace'),
                ('v', 'w', 'claude', 'opus', 'high', 'bypassPermissions', 'v', 'idle', 0, 100, 9000, NULL, 'unused');
    ";

    let store = crate::store::older::store_from_before(dir.path(), crate::store::schema::BEFORE_ACCOUNTS_USED, rows);

    assert_eq!(
        ["ada", "grace", "unused"].map(|id| used_at(&store, id)),
        [Some(2400), Some(600), None],
        "the newest of each account's chats"
    );
    // `unused` was made last, so only a migration that changed the others gives them a newer revision.
    let rev = |id: &str| store.account(&id.into()).unwrap().unwrap().rev;
    assert!(rev("ada") > rev("unused") && rev("grace") > rev("unused"), "one that gets none is as it was");
}
