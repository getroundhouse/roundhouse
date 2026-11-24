//! What a hub keeps of a chat, of the account its agent runs under, and of every other record, which the next hub
//! serves as it was, revisions and all, since a client takes only a copy with a newer revision. The store keeps each
//! part of a record, and its own tests read each back, and a reopened store reads the whole record back as it was, so
//! this restarts a hub once, for everything a chat's first turn and the person leave on its record, rather than once
//! for each.

#![cfg(test)]

use protocol::event::EventBody;
use protocol::model::{Records, Rev};

use crate::accounts::{limits_of, named_account, wait_for_adas_week};
use crate::carrying_on::update;
use crate::context::{after_prompts, wait_for_context};
use crate::prompt_cache::cache_held;
use crate::support::{Stack, wait_for_turn_end};

/// A chat whose agent has had a turn, which said how full its context is, what's cached and what its account's limits
/// are, and which the person read and then marked unread, so it's neither what a reply leaves nor what reading does.
/// The hub that comes back carries on with its agent, and serves the chat's record, its account's and every other as
/// they were, and goes on from them: it still knows when the request before was sent, so it finds that the next one
/// found the cache expired.
#[test]
fn a_chats_record_its_accounts_and_every_other_are_as_they_were_once_the_hub_has_restarted() {
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
    let limits = limits_of(&client, &account.id);
    // Something cached, as the hub says.
    cache_held(&client, &session.id);
    let all_before = client.records().unwrap();

    update(&mut stack);
    let client = stack.client();
    let after = client.session_get(&session.id).unwrap();
    let account_after = named_account(&client, &session.id);
    let limits_after = limits_of(&client, &account.id);
    let all_after = client.records().unwrap();
    client.session_prompt(&session.id, "cold", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);

    assert!(before.unread(), "marked unread");
    assert!(!limits.is_empty(), "its limits heard");
    assert_eq!(after, before, "the chat's record, its revision and all");
    assert_eq!(account_after, Some(account), "the account's record, its revision and all");
    assert_eq!(limits_after, limits, "and its limits");
    assert_eq!(kept(all_after), kept(all_before), "every record the hub keeps, revisions and all");
    let expired = events.iter().filter(|e| matches!(e.body, EventBody::CacheExpired { .. }));
    assert_eq!(expired.count(), 1, "the hub that came back knows when the request before was sent");
}

/// The records a hub keeps from one run to the next, without the list's own revision and its observations, which
/// aren't records: the hub that comes back gives each a revision newer than any a client holds, and reads the
/// workspace's lines again as it takes on its worker, as its own tests say.
fn kept(records: Records) -> Records {
    Records { observations: Vec::new(), rev: Rev::default(), ..records }
}
