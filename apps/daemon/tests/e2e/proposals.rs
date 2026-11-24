//! An agent proposing a decision too large for a question, with the worker's tools: the hub keeps it as a record of
//! its chat, which a client following the chat is told of, and the agent reads it back by its ID, with the person's
//! answer, or their request for another round, which comes to it as a prompt.

#![cfg(test)]

use std::sync::Arc;

use client_core::{Client, OpenSession, RecordsFeed, View};
use protocol::client::proposals::{ProposalAnotherRound, ProposalAnswer};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{
    ClientMessageId, OptionPlace, Proposal, ResponseSummary, RevisionNumber, RevisionStanding, SessionId, ToolUseId,
};
use protocol::prompt::PromptFrom;

use crate::support::{Stack, read_all, session_records, wait_for_turn_end, wait_until};

const PROPOSE: &str = r#"mcp:propose {"title":"Search","context":"Find what was said.","questions":[{"header":"Index","question":"Where does it live?","options":[{"label":"The hub's database","description":"FTS5","body":"**In step** after a crash."},{"label":"The logs","description":"Nothing kept"}],"recommended":"A","why":"It's fast."}]}"#;

/// Each call to a tool in the turn that ended `turn`th, with what it came back with and whether it failed, in order.
fn calls(client: &Client, chat: &SessionId, script: &str, turn: usize) -> Vec<(String, String, bool)> {
    client.session_prompt(chat, script, None).unwrap();
    let events = wait_for_turn_end(client, chat, turn);
    let since = events.iter().rposition(|event| matches!(event.body, EventBody::UserPrompt { .. })).unwrap();
    blocks(&events[since..])
        .filter_map(|block| match block {
            ContentBlock::CallResult { tool_use_id, output, is_error, .. } => {
                Some((tool_use_id.to_string(), client_core::calls::typed::output(output), *is_error))
            }
            _ => None,
        })
        .collect()
}

fn blocks(events: &[SessionEvent]) -> impl Iterator<Item = &ContentBlock> {
    events.iter().flat_map(|event| match &event.body {
        EventBody::AgentUser { blocks, .. } => blocks.as_slice(),
        _ => &[],
    })
}

fn proposals(feed: &RecordsFeed, chat: &SessionId) -> Vec<Arc<Proposal>> {
    feed.snapshot().proposals(chat).to_vec()
}

#[test]
fn a_proposal_is_kept_for_the_call_that_made_it_shown_to_a_client_following_its_chat_and_read_back() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    let feed = RecordsFeed::start(&client);
    // The chat open, as a client that shows it has it, which follows its session, and with it the session's records.
    let _open = OpenSession::start(&client, &feed, &chat, View::Own);
    wait_until("the feed to follow", || feed.connected());

    let proposed = calls(&client, &chat.id, PROPOSE, 1);

    let [(call, text, false)] = proposed.as_slice() else { panic!("one call that worked: {proposed:?}") };
    wait_until("the feed to be told of it", || !proposals(&feed, &chat.id).is_empty());
    let [kept] = proposals(&feed, &chat.id).try_into().unwrap();
    assert!(text.starts_with(&format!("Proposed \"Search\" as {}.", kept.id)), "{text}");
    assert_eq!(kept.current.call.as_ref().map(ToolUseId::as_str), Some(call.as_str()), "drawn as its call's line");
    let question = &kept.current.proposed.questions[0];
    let recommended = question.recommended.as_ref().and_then(|r| question.letter(r.option)).map(|l| l.to_string());
    assert_eq!((question.options.len(), recommended.as_deref()), (2, Some("A")));

    let read = calls(&client, &chat.id, &format!(r#"mcp:get_proposal {{"proposal":"{}"}}"#, kept.id), 2);

    let [(_, text, false)] = read.as_slice() else { panic!("one call that worked: {read:?}") };
    assert!(text.contains("A. The hub's database: FTS5\n**In step** after a crash."), "{text}");
    assert!(text.contains("You recommended A. It's fast."), "{text}");
}

#[test]
fn a_proposal_the_hub_refuses_says_why_and_keeps_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    let one_option = r#"mcp:propose {"title":"Search","questions":[{"header":"Index","question":"Where?","options":[{"label":"The hub"}]}]}"#;

    let refused = calls(&client, &chat.id, one_option, 1);

    let [(_, text, true)] = refused.as_slice() else { panic!("one call that failed: {refused:?}") };
    assert!(text.starts_with("Roundhouse couldn't make the proposal:"), "{text}");
    assert!(session_records(&client, &chat.id).proposals.is_empty());
}

/// The person's answer is kept on the proposal, the agent is sent a prompt that says what was chosen, once however
/// often it's sent, and it reads the note with its tool.
#[test]
fn an_answer_tells_the_agent_what_was_chosen_once_and_the_agent_reads_the_note() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    calls(&client, &chat.id, PROPOSE, 1);
    let [proposal] = session_records(&client, &chat.id).proposals.try_into().unwrap();
    let answer = |chosen, note: &str, request: &str| {
        client.proposal_answer(ProposalAnswer {
            id: proposal.id.clone(),
            revision: proposal.current.number,
            chosen: vec![OptionPlace(chosen)],
            note: note.into(),
            client_request_id: request.into(),
        })
    };

    let answered = answer(1, "Keep it small.", "r1").unwrap();
    wait_for_turn_end(&client, &chat.id, 2);
    let again = answer(1, "Keep it small.", "r1").unwrap();
    let events = read_all(&client, &chat.id);

    assert_eq!(again, answered, "the same request reads it back");
    let told: Vec<(Option<&str>, &PromptFrom, &str)> = events
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::UserPrompt { client_msg_id, prompt, from } if prompt.text.starts_with("I chose") => {
                Some((client_msg_id.as_ref().map(ClientMessageId::as_str), from, prompt.text.as_str()))
            }
            _ => None,
        })
        .collect();
    let expected = format!(
        "I chose B, The logs, in your proposal \"Search\" ({}). I wrote a note with it, which get_proposal reads.",
        proposal.id
    );
    let [(message, from, said)] = told.as_slice() else { panic!("the agent is told once: {told:?}") };
    let for_it = PromptFrom::ProposalResponse { proposal: proposal.id.clone(), revision: RevisionNumber::FIRST };
    assert_eq!(*from, &for_it, "by the hub, for the proposal's revision");
    assert_eq!(*said, expected);
    assert!(message.is_some_and(|message| message != "r1"), "an ID of the hub's own, not the request's: {message:?}");
    let otherwise = answer(0, "", "r2").unwrap_err();
    assert!(otherwise.to_string().contains("answered already"), "{otherwise}");

    let read = calls(&client, &chat.id, &format!(r#"mcp:get_proposal {{"proposal":"{}"}}"#, proposal.id), 3);

    let [(_, text, false)] = read.as_slice() else { panic!("one call that worked: {read:?}") };
    assert!(text.ends_with("The person chose B, The logs, and wrote:\nKeep it small."), "{text}");
}

/// The person's request for another round is kept on the proposal, the agent is told of it in a prompt, reads the note
/// with its tool, and revises, and the revision waits for the person.
#[test]
fn another_round_tells_the_agent_which_reads_the_note_and_revises() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, chat) = stack.workspace(&client);
    calls(&client, &chat.id, PROPOSE, 1);
    let [proposal] = session_records(&client, &chat.id).proposals.try_into().unwrap();
    let round = ProposalAnotherRound {
        id: proposal.id.clone(),
        revision: proposal.current.number,
        note: "Could the logs keep an index of their own?".into(),
        client_request_id: "r1".into(),
    };

    client.proposal_another_round(round).unwrap();
    wait_for_turn_end(&client, &chat.id, 2);
    let events = read_all(&client, &chat.id);
    let read = calls(&client, &chat.id, &format!(r#"mcp:get_proposal {{"proposal":"{}"}}"#, proposal.id), 3);
    let revise = PROPOSE.replace(
        "mcp:propose {",
        &format!(r#"mcp:revise_proposal {{"proposal":"{}","changed":"Gave the logs an index.","#, proposal.id),
    );
    let revised = calls(&client, &chat.id, &revise, 4);

    let told = events.iter().find_map(|event| match &event.body {
        EventBody::UserPrompt { prompt, from: PromptFrom::ProposalResponse { .. }, .. } => Some(prompt.text.as_str()),
        _ => None,
    });
    assert!(told.is_some_and(|told| told.starts_with("I'd like another round of your proposal")), "{told:?}");
    let [(_, text, false)] = read.as_slice() else { panic!("one call that worked: {read:?}") };
    assert!(text.ends_with("and wrote what to change:\nCould the logs keep an index of their own?"), "{text}");
    let [(_, _, false)] = revised.as_slice() else { panic!("one call that worked: {revised:?}") };
    let [newest] = session_records(&client, &chat.id).proposals.try_into().unwrap();
    assert_eq!((newest.current.number, newest.current.standing()), (RevisionNumber(2), RevisionStanding::Waiting));
    assert!(matches!(newest.revisions[0].response, Some(ResponseSummary::AnotherRound { .. })), "{newest:?}");
}
