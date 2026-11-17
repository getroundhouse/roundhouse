//! What a hub keeps of a chat, and of the account its agent runs under, which the next hub serves as it was. The
//! store keeps each part of a record, and its own tests read each back, and a reopened store reads the whole record
//! back as it was, so this restarts a hub once, for everything a chat's first turn and the person leave on its record,
//! rather than once for each.

#![cfg(test)]

use protocol::event::EventBody;
use protocol::model::Session;

use crate::accounts::{named_account, wait_for_adas_week};
use crate::context::{after_prompts, wait_for_context};
use crate::support::{Stack, wait_for_turn_end};

/// A chat whose agent has had a turn, which said how full its context is, what's cached and what its account's limits
/// are, and which the person read and then marked unread, so it's neither what a reply leaves nor what reading does.
/// The hub that comes back serves the chat's record and its account's as they were, and goes on from them: it still
/// knows when the request before was sent, so it finds that the next one found the cache expired.
#[test]
fn a_chats_record_and_its_accounts_are_as_they_were_once_the_hub_has_restarted() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_context(&client, &session.id, &after_prompts(1));
    wait_for_adas_week(&client, &session.id);
    let replied = client.session_get(&session.id).unwrap();
    client.session_mark_read(&session.id, replied.reply_seq).unwrap();
    let before = client.session_mark_unread(&session.id).unwrap();
    let account = named_account(&client, &session.id).expect("the chat names its account");

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let after = client.session_get(&session.id).unwrap();
    let account_after = named_account(&client, &session.id);
    client.session_prompt(&session.id, "cold", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);

    assert!(before.unread(), "marked unread");
    assert!(before.prompt_cache.is_some(), "something cached");
    // All but its revision, which the hub that comes back moves though nothing of the record changes. That's against
    // the rule that a revision moves only when the record does, which is #1774, and it's not what this is about.
    assert_eq!(Session { rev: before.rev, ..after }, before, "the chat's record");
    assert_eq!(account_after, Some(account), "the account's record, its limits and all");
    let expired = events.iter().filter(|e| matches!(e.body, EventBody::CacheExpired { .. }));
    assert_eq!(expired.count(), 1, "the hub that came back knows when the request before was sent");
}
