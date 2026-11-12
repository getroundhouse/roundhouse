use protocol::event::{CompactedBy, CompactionOutcome, ContentBlock, EventBody};
use protocol::model::Workspace;
use rusqlite::params;

use super::*;
use crate::store::events::tests::prompt;
use crate::store::tests::{idle_session, notice, store_with_session, workspace_w};

/// A message from a subagent, which isn't among the agent's own events, so what follows it isn't placed at its `seq`.
fn subagents_message() -> EventBody {
    EventBody::Assistant {
        message_id: None,
        parent_tool_use_id: Some("call-1".into()),
        model: None,
        blocks: vec![ContentBlock::Text { text: "x".into(), images: Vec::new() }],
        first_index: None,
    }
}

#[test]
fn every_prompt_is_listed_in_order_as_a_line_and_nothing_else_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let append = |body: &EventBody| store.append(&"s".into(), None, body, None, None, None).unwrap();

    append(&prompt("Fix\n  this", 0));
    append(&EventBody::PromptTaken { prompt: 1 });
    append(&notice("x"));
    // A subagent's, which isn't among the agent's own, so the next prompt's place isn't its `seq`.
    append(&subagents_message());
    append(&prompt("", 2));

    let placed = |seq, text, files, place| PromptLine::new(seq, text, files).placed(Place::new(place));
    assert_eq!(store.prompt_lines(&"s".into()).unwrap(), [placed(1, "Fix this", 0, 1), placed(5, "", 2, 4)]);
    assert!(store.prompt_lines(&"other".into()).unwrap().is_empty());
}

fn compaction(by: CompactedBy, tokens: Option<(u64, u64)>) -> EventBody {
    let outcome = CompactionOutcome::Compacted {
        by,
        before_tokens: tokens.map(|(before, _)| before),
        after_tokens: tokens.map(|(_, after)| after),
        took_ms: Some(51_558),
        summary: Some("What the agent carries on from. ".repeat(100)),
    };
    EventBody::AgentCompacted { asked: None, keeping: None, outcome }
}

/// The hub reads a compaction's line straight from what's stored, and a client makes its line from the event, so the
/// two must agree, or a client would mark the same compaction twice.
#[test]
fn every_compaction_that_compacted_is_listed_in_order_as_its_event_makes_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let append = |body: &EventBody| store.append(&"s".into(), None, body, None, None, None).unwrap().event;

    append(&prompt("Fix this", 0));
    // A subagent's, which isn't among the agent's own, so the compaction after it isn't placed at its `seq`.
    append(&subagents_message());
    let by_itself = append(&compaction(CompactedBy::Agent, Some((467_499, 15_694))));
    append(&EventBody::CompactionAsked { keeping: None });
    let failed = CompactionOutcome::Failed { error: Some("Not enough messages to compact.".to_string()) };
    append(&EventBody::AgentCompacted { asked: Some(4), keeping: None, outcome: failed });
    let asked = append(&compaction(CompactedBy::Asked, None));
    append(&EventBody::AgentCompacting { written_chars: Some(16_464) });

    let made: Vec<CompactionLine> = [by_itself, asked].iter().map(|event| CompactionLine::of(event).unwrap()).collect();
    assert_eq!(store.compaction_lines(&"s".into()).unwrap(), made);
    assert_ne!(made[0].place.get(), made[0].seq, "it's at its place among the agent's own");
    assert!(store.compaction_lines(&"other".into()).unwrap().is_empty());
}

/// Two chats in a workspace are worked in by turns, so their prompts are read in the order they were sent, not chat by
/// chat.
#[test]
fn a_workspaces_prompts_are_listed_oldest_first_across_its_chats() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let chat = |id: &str, workspace: &str| {
        let mut chat = idle_session(1);
        (chat.session.id, chat.session.workspace_id) = (id.into(), workspace.into());
        chat.agent_session_id = id.into();
        store.insert_session(&chat, None).unwrap();
    };
    let elsewhere = Workspace {
        id: "w2".into(),
        name: "birch".into(),
        branch: "birch".into(),
        instance_id: "w2".into(),
        ..workspace_w(1)
    };
    store.insert_workspace(&elsewhere, None).unwrap();
    chat("second", "w");
    chat("other", "w2");
    let sent = |chat: &str, at_ms: i64, text: &str| {
        let seq = store.append(&chat.into(), None, &prompt(text, 0), None, None, None).unwrap().event.seq;
        let when = "UPDATE events SET ts_ms = ?1 WHERE session_id = ?2 AND seq = ?3";
        lock(&store.writer).execute(when, params![at_ms, chat, seq.cast_signed()]).unwrap();
    };

    sent("s", 10, "Fix the login");
    sent("s", 30, "Add a test");
    sent("second", 20, "Why does\nit loop?");
    store.append(&"second".into(), None, &notice("x"), None, None, None).unwrap();
    sent("other", 15, "Write the docs");

    let listed = store.workspace_prompt_lines(&"w".into()).unwrap();
    let texts: Vec<&str> = listed.iter().map(PromptLine::text).collect();
    assert_eq!(texts, ["Fix the login", "Why does it loop?", "Add a test"]);
    assert!(store.workspace_prompt_lines(&"missing".into()).unwrap().is_empty());
}

/// The hub reads a prompt's text straight from what's stored, and a client makes its line from the event, so the two
/// must agree, or a client would count the same prompt as changed.
#[test]
fn a_listed_line_is_the_line_its_event_makes() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let long = format!("{}{}", "\n".repeat(300), "word ".repeat(100));
    let texts =
        ["\0a\0b", "\u{1b}[2Jcleared", "abc\u{202E}def", "\"quoted\" \\ back", "👩\u{200D}💻 é", long.as_str(), ""];

    let made: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(files, text)| {
            let event = store.append(&"s".into(), None, &prompt(text, files), None, None, None).unwrap().event;
            PromptLine::of(&event).unwrap()
        })
        .collect();

    assert_eq!(store.prompt_lines(&"s".into()).unwrap(), made);
}
