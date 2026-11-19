//! How many chats are at work on each account, which Automatic shares an account's usage among.

use super::*;
use crate::store::tests::idle_session;
use protocol::model::{AccountSource, SessionStatus};

/// Adds the chat `id` on `account`, through `source`, with `status`, and closed if `closed`.
fn chat(store: &Store, id: &str, account: &str, source: AccountSource, status: SessionStatus, closed: bool) {
    let mut stored = idle_session(0);
    stored.agent_session_id = Some(id.into());
    let session = &mut stored.session;
    (session.id, session.account, session.account_source, session.status) =
        (id.into(), Some(account.into()), Some(source), status);
    session.closed_at_ms = closed.then_some(1);
    store.insert_session(&stored, None).unwrap();
}

#[test]
fn only_open_chats_on_the_hubs_login_for_an_account_on_a_turn_now_are_at_work_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let (hub, ambient) = (AccountSource::Hub, AccountSource::Ambient);
    chat(&store, "running", "work", hub, SessionStatus::Running, false);
    chat(&store, "asking", "work", hub, SessionStatus::WaitingApproval, false);
    chat(&store, "idle", "work", hub, SessionStatus::Idle, false);
    chat(&store, "closed", "work", hub, SessionStatus::Running, true);
    chat(&store, "ambient", "work", ambient, SessionStatus::Running, false);
    chat(&store, "personal", "personal", hub, SessionStatus::Running, false);

    let at_work = store.at_work_on_accounts().unwrap();

    let expected = [(AccountId::from("work"), 2), (AccountId::from("personal"), 1)].into_iter().collect();
    assert_eq!(at_work, expected);
}
