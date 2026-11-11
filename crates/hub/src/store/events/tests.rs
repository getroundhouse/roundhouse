use protocol::attachment::{Attachment, MediaType};
use protocol::model::SessionStatus::{Idle, Running, WaitingApproval};
use protocol::model::{AttachmentId, Session, SessionStatus, Workspace};

use super::*;
use crate::store::tests::{idle_session, notice, store_with_session, workspace_w};

/// Appends an event that changes the session's status.
fn change(store: &Store, status: SessionStatus, turn: TurnEdge) -> SessionEvent {
    let change = Some(StatusChange { status, turn, background_tasks: None }.into());
    store.append(&"s".into(), None, &notice("x"), None, None, change).unwrap().event
}

fn session(store: &Store) -> Session {
    store.get_session(&"s".into()).unwrap().session
}

#[test]
fn a_turn_starts_when_the_event_that_starts_it_was_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(session(&store).turn_started_at_ms, None, "a new session hasn't had a turn");

    let prompt = change(&store, Running, TurnEdge::Starts);
    change(&store, WaitingApproval, TurnEdge::Neither);
    change(&store, Running, TurnEdge::Neither);
    change(&store, Idle, TurnEdge::Ends);

    assert_eq!(session(&store).turn_started_at_ms, Some(prompt.ts_ms), "it stays the newest turn's once it's over");
}

#[test]
fn the_next_turn_moves_it_and_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    change(&store, Running, TurnEdge::Starts);
    change(&store, Idle, TurnEdge::Ends);
    // Earlier than any clock could say, so the next turn can't land on the same moment.
    lock(&store.writer).execute("UPDATE sessions SET turn_started_at_ms = 1", []).unwrap();
    let before = session(&store);

    let prompt = change(&store, Running, TurnEdge::Starts);

    let after = session(&store);
    assert_eq!(after.turn_started_at_ms, Some(prompt.ts_ms));
    assert!(after.rev > before.rev, "a client only takes a newer copy");
}

fn prompt(text: &str, files: usize) -> EventBody {
    let shot = |n| Attachment {
        id: AttachmentId::from(format!("a{n}")),
        name: "shot.png".to_string(),
        media_type: MediaType::Png,
        size_bytes: 4,
    };
    EventBody::UserPrompt { text: text.to_string(), client_msg_id: None, attachments: (0..files).map(shot).collect() }
}

#[test]
fn every_prompt_is_listed_in_order_as_a_line_and_nothing_else_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let append = |body: &EventBody| store.append(&"s".into(), None, body, None, None, None).unwrap();

    append(&prompt("Fix\n  this", 0));
    append(&EventBody::PromptTaken { prompt: 1 });
    append(&notice("x"));
    append(&prompt("", 2));

    assert_eq!(
        store.prompt_lines(&"s".into()).unwrap(),
        [PromptLine::new(1, "Fix this", 0), PromptLine::new(4, "", 2)]
    );
    assert!(store.prompt_lines(&"other".into()).unwrap().is_empty());
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
    let elsewhere = Workspace { id: "w2".into(), name: "birch".into(), branch: "birch".into(), ..workspace_w(1) };
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
            PromptLine::of(event.seq, &event.body).unwrap()
        })
        .collect();

    assert_eq!(store.prompt_lines(&"s".into()).unwrap(), made);
}
