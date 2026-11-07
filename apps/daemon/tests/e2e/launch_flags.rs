//! Launch flags disagreeing with the CLI's records.

#![cfg(test)]

use protocol::event::EventBody;

use crate::support::{Stack, assistant_text, wait_for_turn_end};

#[test]
fn a_deleted_conversation_starts_fresh_with_a_warning() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    drop(client);
    stack.kill_hub();
    // The hub thinks this conversation exists, because it saw it open.
    std::fs::remove_file(stack.agent_state().join(session.id.as_str())).unwrap();

    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:second", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).ends_with("second"));
    let warned =
        events.iter().any(|e| matches!(&e.body, EventBody::Notice { message, .. } if message.contains("not found")));
    assert!(warned, "{events:#?}");
}

#[test]
fn a_conversation_the_hub_did_not_know_had_opened_is_continued() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    drop(client);
    stack.kill_hub();
    // As if a launch saved the conversation, then crashed before the hub heard about it.
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join(session.id.as_str()), "say:from before the crash\n").unwrap();

    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(assistant_text(&events).contains("from before the crash"), "{events:#?}");
}
