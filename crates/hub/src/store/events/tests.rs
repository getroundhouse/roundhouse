use std::collections::HashMap;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::attachment::{Attachment, MediaType};
use protocol::event::ContentBlock;
use protocol::model::SessionStatus::{Idle, Running, WaitingApproval};
use protocol::model::{AttachmentId, Session, SessionStatus, Workspace};

use super::*;
use crate::store::tests::{idle_session, notice, store_with_session, workspace_w};
use crate::store::{older, schema};

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

/// Each event, whether it was from a run, and whether recording it is when the person was last active.
#[test]
fn a_prompt_or_an_answer_from_the_person_is_when_they_were_last_active_and_nothing_else_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let answer = |outcome| EventBody::ApprovalResolved { approval_id: "a".into(), outcome, answers: None };
    let sent = [
        (None, prompt("go", 0), true),
        (None, answer(ApprovalOutcome::Allowed), true),
        (None, answer(ApprovalOutcome::Denied), true),
        (None, answer(ApprovalOutcome::Cancelled), false),
        (Some(RunId::new(1)), answer(ApprovalOutcome::Allowed), false),
        (None, notice("x"), false),
    ];
    for (run, body, by_the_person) in sent {
        // Earlier than any clock could say, so a change can't land on the same moment.
        let forget = "UPDATE sessions SET user_active_at_ms = 1; UPDATE workspaces SET user_active_at_ms = 1";
        lock(&store.writer).execute_batch(forget).unwrap();

        let appended = store.append(&"s".into(), run, &body, None, None, None).unwrap();

        let at = Some(appended.event.ts_ms);
        assert_eq!(session(&store).user_active_at_ms == at, by_the_person, "{body:?}");
        assert_eq!(
            store.get_workspace(&"w".into()).unwrap().user_active_at_ms == at,
            by_the_person,
            "and its workspace"
        );
        assert_eq!(appended.by_the_person, by_the_person, "{body:?}");
    }
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
    // A subagent's, which isn't among the agent's own, so the next prompt's place isn't its `seq`.
    append(&Sent::Message(Some(1)).body());
    append(&prompt("", 2));

    let placed = |seq, text, files, place| PromptLine::new(seq, text, files).placed(Place::new(place));
    assert_eq!(store.prompt_lines(&"s".into()).unwrap(), [placed(1, "Fix this", 0, 1), placed(5, "", 2, 4)]);
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

/// An event someone sent: the hub, the agent, or one of a few subagents, by the number of the call that started it.
#[derive(Clone, Copy, Debug)]
enum Sent {
    Hub,
    Message(Option<u8>),
    Result(Option<u8>),
    /// A line with no normalized form, such as a synthetic `user` line.
    Other(Option<u8>),
    /// The agent's own word on a call that's taking a while, whose line names the call as its parent.
    Progress,
}

impl Sent {
    fn subagent(self) -> Option<ToolUseId> {
        match self {
            Sent::Hub | Sent::Progress => None,
            Sent::Message(from) | Sent::Result(from) | Sent::Other(from) => {
                from.map(|call| format!("call-{call}").into())
            }
        }
    }

    fn body(self) -> EventBody {
        let parent_tool_use_id = self.subagent();
        match self {
            Sent::Hub => notice("x"),
            Sent::Message(_) => EventBody::Assistant {
                message_id: None,
                parent_tool_use_id,
                model: None,
                blocks: vec![ContentBlock::Text { text: "x".into(), images: Vec::new() }],
                first_index: None,
            },
            Sent::Result(_) => EventBody::AgentUser { parent_tool_use_id, blocks: Vec::new() },
            Sent::Other(_) => EventBody::AgentOther { kind: "user/synthetic".into(), parent_tool_use_id },
            Sent::Progress => EventBody::AgentOther { kind: "tool_progress".into(), parent_tool_use_id: None },
        }
    }

    /// The event as a build from before subagents were recorded wrote it: an `AgentOther` named its subagent only in
    /// the agent's own line.
    fn before_subagents(self, session: &str, seq: usize) -> String {
        let (body, raw) = match self {
            Sent::Other(_) => {
                let raw =
                    serde_json::json!({"type": "user", "isSynthetic": true, "parent_tool_use_id": self.subagent()});
                (EventBody::AgentOther { kind: "user/synthetic".into(), parent_tool_use_id: None }, raw.to_string())
            }
            Sent::Progress => {
                let raw = serde_json::json!({"type": "tool_progress", "parent_tool_use_id": "call-0"});
                (self.body(), raw.to_string())
            }
            Sent::Hub | Sent::Message(_) | Sent::Result(_) => (self.body(), "{}".into()),
        };
        let (kind, body) = (body.kind(), serde_json::to_string(&body).unwrap());
        format!(
            "INSERT INTO events (session_id, seq, ts_ms, kind, body, raw) \
             VALUES ('{session}', {seq}, 0, '{kind}', '{body}', '{raw}');"
        )
    }
}

fn sent() -> impl Strategy<Value = Sent> {
    let from = prop::option::of(0..3u8);
    prop_oneof![
        Just(Sent::Hub),
        from.clone().prop_map(Sent::Message),
        from.clone().prop_map(Sent::Result),
        from.prop_map(Sent::Other),
        Just(Sent::Progress),
    ]
}

/// Checks that each of `events`, which `sent` are in order, names its subagent, and takes the next place among the
/// events from the same one.
fn assert_placed(events: &[SessionEvent], sent: &[Sent]) -> Result<(), TestCaseError> {
    prop_assert_eq!(events.len(), sent.len());
    let mut counts = HashMap::new();
    for (event, sent) in events.iter().zip(sent) {
        let count = counts.entry(sent.subagent()).or_insert(0);
        *count += 1;
        prop_assert_eq!(&event.subagent, &sent.subagent(), "event {}", event.seq);
        prop_assert_eq!(event.place, Place::new(*count), "event {}", event.seq);
    }
    Ok(())
}

proptest! {
    #![proptest_config(Config {
        cases: 64,
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn each_event_takes_the_next_place_among_those_from_its_subagent(sent in prop::collection::vec(sent(), 0..40)) {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_session(dir.path());

        let append = |sent: &Sent| store.append(&"s".into(), None, &sent.body(), None, None, None).unwrap().event;
        let appended: Vec<_> = sent.iter().map(append).collect();

        assert_placed(&appended, &sent)?;
        prop_assert_eq!(store.read_events(&"s".into(), 1, u32::MAX, u64::MAX, false).unwrap(), appended);
    }

    #[test]
    fn a_migrated_log_names_each_events_subagent_and_counts_its_places(
        sent in prop::collection::vec((any::<bool>(), sent()), 0..40),
        next in sent(),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let sessions = [("s", true), ("t", false)].map(|(id, first)| {
            let sent: Vec<Sent> =
                sent.iter().filter(|(of_first, _)| *of_first == first).map(|(_, sent)| *sent).collect();
            (id, sent)
        });
        let rows: String = sessions
            .iter()
            .flat_map(|(id, sent)| {
                let session = format!(
                    "INSERT INTO sessions \
                     (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms, updated_at_ms) \
                     VALUES ('{id}', 'w', 'claude', '{id}', 'idle', {}, 0, 0);",
                    sent.len()
                );
                std::iter::once(session).chain((1..).zip(sent).map(|(seq, sent)| sent.before_subagents(id, seq)))
            })
            .collect();

        let store = older::store_from_before(dir.path(), schema::BEFORE_SUBAGENTS, &rows);

        for (id, sent) in &sessions {
            assert_placed(&store.read_events(&(*id).into(), 1, u32::MAX, u64::MAX, false).unwrap(), sent)?;
        }
        let (_, mut sent) = sessions.into_iter().next().unwrap();
        store.append(&"s".into(), None, &next.body(), None, None, None).unwrap();
        sent.push(next);
        assert_placed(&store.read_events(&"s".into(), 1, u32::MAX, u64::MAX, false).unwrap(), &sent)?;
    }
}

#[test]
fn only_a_user_line_that_names_a_call_is_a_subagents_once_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let other = r#"'{"type":"agent_other","kind":"x"}'"#;
    let rows = format!(
        r#"
        INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms, updated_at_ms)
            VALUES ('s', 'w', 'claude', 's', 'idle', 5, 0, 0);
        INSERT INTO events (session_id, seq, ts_ms, kind, body, raw) VALUES
            ('s', 1, 0, 'agent_other', {other}, 'Warning: not JSON'),
            ('s', 2, 0, 'agent_other', {other}, NULL),
            ('s', 3, 0, 'agent_other', {other}, '{{"type":"user","parent_tool_use_id":7}}'),
            ('s', 4, 0, 'agent_other', {other}, '{{"type":"tool_progress","parent_tool_use_id":"bash"}}'),
            ('s', 5, 0, 'agent_other', {other}, '{{"type":"user","parent_tool_use_id":"call"}}');
        "#
    );

    let store = older::store_from_before(dir.path(), schema::BEFORE_SUBAGENTS, &rows);

    let events = store.read_events(&"s".into(), 1, 10, u64::MAX, false).unwrap();
    let placed: Vec<_> =
        events.iter().map(|event| (event.subagent.as_ref().map(ToolUseId::as_str), event.place.get())).collect();
    assert_eq!(placed, [(None, 1), (None, 2), (None, 3), (None, 4), (Some("call"), 1)]);
}

/// Rebuilding the log keeps what the migration doesn't work out: each event's time, the client's id for it, its run
/// and its line, and every index the log had.
#[test]
fn a_migrated_log_keeps_each_events_time_id_run_and_line_and_its_indexes() {
    let indexes = |conn: &Connection| {
        let mut indexes =
            conn.prepare("SELECT name, sql FROM sqlite_master WHERE type = 'index' AND tbl_name = 'events'").unwrap();
        let read = indexes.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)));
        read.unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let before = tempfile::tempdir().unwrap();
    let mut conn = schema::open_connection(&before.path().join("hub.db")).unwrap();
    schema::migrate_to(&mut conn, &before.path().join("backups"), schema::BEFORE_SUBAGENTS).unwrap();
    let had = indexes(&conn);
    let dir = tempfile::tempdir().unwrap();
    let rows = r#"
        INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms, updated_at_ms)
            VALUES ('s', 'w', 'claude', 's', 'idle', 2, 0, 0);
        INSERT INTO events (session_id, seq, ts_ms, kind, client_msg_id, run, body, raw) VALUES
            ('s', 1, 1700, 'notice', 'msg-1', 3, '{"type":"notice","level":"info","message":"a"}', 'line a'),
            ('s', 2, 1800, 'notice', NULL, NULL, '{"type":"notice","level":"info","message":"b"}', NULL);
    "#;

    let store = older::store_from_before(dir.path(), schema::BEFORE_SUBAGENTS, rows);

    let events = store.read_events(&"s".into(), 1, 10, u64::MAX, true).unwrap();
    let kept: Vec<_> = events.iter().map(|event| (event.ts_ms, event.run, event.raw.as_deref())).collect();
    assert_eq!(kept, [(1700, Some(RunId::new(3)), Some("line a")), (1800, None, None)]);
    let again = store.append(&"s".into(), None, &notice("c"), None, Some(&"msg-1".into()), None).unwrap();
    assert!(again.duplicate, "sending again what was sent before the migration adds nothing");
    assert_eq!(again.event, SessionEvent { raw: None, ..events[0].clone() });
    let has = indexes(&schema::open_connection(&dir.path().join("hub.db")).unwrap());
    let lost: Vec<_> = had.iter().filter(|index| !has.contains(index)).collect();
    assert!(lost.is_empty(), "lost {lost:?}, and has {has:?}");
    assert!(has.iter().any(|(name, _)| name == "events_by_subagent"), "{has:?}");
}

#[test]
fn a_read_by_place_takes_one_agent_or_subagents_events_up_to_a_seq_and_says_how_far_they_go() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for sent in [Sent::Message(Some(0)), Sent::Hub, Sent::Result(Some(0)), Sent::Other(Some(1)), Sent::Other(Some(0))] {
        store.append(&"s".into(), None, &sent.body(), None, None, None).unwrap();
    }
    let call = Sent::Message(Some(0)).subagent();
    let read = |from, through, limit| {
        let places = Places { subagent: call.as_ref(), from: Place::new(from), through };
        let (events, last) = store.read_places(&"s".into(), places, limit, u64::MAX, false).unwrap();
        (events.iter().map(|event| (event.seq, event.place.get())).collect::<Vec<_>>(), last.get())
    };

    assert_eq!(read(1, None, 10), (vec![(1, 1), (3, 2), (5, 3)], 3));
    assert_eq!(read(2, None, 1), (vec![(3, 2)], 3));
    assert_eq!(read(2, Some(4), 10), (vec![(3, 2)], 3), "nothing recorded after the seq it stops at");
    assert_eq!(read(4, None, 10), (vec![], 3));
}
