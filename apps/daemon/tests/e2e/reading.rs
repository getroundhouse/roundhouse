//! Which chats have a reply nobody has read.

#![cfg(test)]

use client_core::RecordsFeed;
use protocol::model::{Session, SessionId};

use crate::support::{Stack, wait_for_turn_end, wait_until};

/// Whether client-core's copy says this session has a reply nobody has read.
fn unread_in(feed: &RecordsFeed, id: &SessionId) -> bool {
    feed.snapshot().session(id).is_some_and(Session::unread)
}

/// That a chat marked unread is still unread once the hub has restarted is in `records_kept.rs`, with the rest of its
/// record.
#[test]
fn a_reply_stays_unread_until_someone_reads_it() {
    let stack = Stack::start();
    let client = stack.client();
    // On a connection of its own, so only the feed, and not the replies it would take in, can say it's read.
    let feed = RecordsFeed::start(&stack.client());
    let (_, session) = stack.workspace(&client);
    let (prompt, _) = client.session_prompt(&session.id, "say:hello", None).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);

    let replied = client.session_get(&session.id).unwrap();
    assert!(replied.unread());
    assert!(replied.reply_seq > prompt && replied.reply_seq <= events.last().unwrap().seq);
    wait_until("the feed to say it's unread", || unread_in(&feed, &session.id));
    assert!(!client.session_mark_read(&session.id, replied.reply_seq).unwrap().unread());
    wait_until("the feed to say it's read", || !unread_in(&feed, &session.id));

    assert!(client.session_mark_unread(&session.id).unwrap().unread());
}
