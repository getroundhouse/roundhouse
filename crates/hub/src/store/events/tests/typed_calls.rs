//! What the migration that typed Claude's calls made of what the hub had kept.

use protocol::capability::calls::call::{Command, Read, Runs};
use protocol::capability::calls::output::Lines;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{Asking, ContentBlock, EventBody};
use protocol::model::{LatelyCall, Seq, SessionId};
use serde_json::{Value, json};

use crate::store::{Store, older, schema};

/// A database as the build before the migration left it, with the chats `s` and `t`, each holding `bodies`, each an
/// event's body as JSON, in order, and a working subagent of `s`'s whose record kept `recent`.
fn migrated(dir: &std::path::Path, bodies: &[Value], recent: &Value) -> Store {
    let quoted = |json: String| json.replace('\'', "''");
    let rows: String = ["s", "t"]
        .into_iter()
        .flat_map(|session| {
            let chat = format!(
                "INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms, \
                 updated_at_ms) VALUES ('{session}', 'w', 'claude', '{session}', 'idle', {}, 0, 0);",
                bodies.len()
            );
            let events = (1..).zip(bodies).map(move |(seq, body)| {
                format!(
                    "INSERT INTO events (session_id, seq, ts_ms, kind, place, body) \
                     VALUES ('{session}', {seq}, 0, '{}', {seq}, '{}');",
                    body["type"].as_str().unwrap(),
                    quoted(body.to_string()),
                )
            });
            std::iter::once(chat).chain(events)
        })
        .chain(std::iter::once(format!(
            "INSERT INTO subagents (session_id, call, id, status, started_at_ms, recent) \
             VALUES ('s', 'agent', 'agent', 'working', 1, '{}');",
            quoted(recent.to_string())
        )))
        .collect();
    older::store_from_before(dir, schema::BEFORE_TYPED_CALLS, &rows)
}

fn bodies(store: &Store, session: &str) -> Vec<EventBody> {
    let events = store.read_events(&SessionId::from(session), Seq::new(1), 100, u64::MAX, false).unwrap();
    events.into_iter().map(|event| event.body).collect()
}

fn ls() -> Call {
    Call::Command(Command {
        command: "ls".into(),
        cwd: None,
        description: None,
        timeout_ms: None,
        runs: Runs::Waits,
        parts: Vec::new(),
        unsandboxed: None,
    })
}

/// Every chat's log is typed, each result against the call it answers, which came in an event before it.
#[test]
fn a_migrated_log_has_claude_s_calls_and_what_came_back_for_each_typed() {
    let dir = tempfile::tempdir().unwrap();
    let read = json!({"type":"tool_use","id":"read","name":"Read","input":{"file_path":"/a.rs"}});
    let lines = json!({"type":"tool_result","tool_use_id":"read","is_error":false,"text":"     1\tfn a() {}",
        "call":{"kind":"read"}});
    let request = json!({"type":"approval_requested","approval_id":"a","tool_name":"Bash","tool_use_id":"ls",
        "input":{"command":"ls"},"description":"It's 'here'","asking":{"kind":"tool_use","call":{"kind":"command"}}});
    let other = json!({"type":"agent_other","kind":"tool_use"});
    let log = [
        json!({"type":"assistant","blocks":[{"type":"text","text":"I'll read it."},read]}),
        json!({"type":"agent_user","blocks":[lines]}),
        request,
        other.clone(),
    ];
    let recent = json!({"calls":[{"call":{"id":"ls","name":"Bash","input":{"command":"ls"}}}]});

    let store = migrated(dir.path(), &log, &recent);

    let made = Call::Read(Read { path: "/a.rs".into(), lines: None, pages: None });
    let came = CallOutput::Lines(Lines { first: 1, lines: vec!["fn a() {}".into()], total: None });
    for session in ["s", "t"] {
        let [
            EventBody::Assistant { blocks: asked, .. },
            EventBody::AgentUser { blocks: answered, .. },
            requested,
            kept,
        ] = <[EventBody; 4]>::try_from(bodies(&store, session)).unwrap()
        else {
            panic!("not the events recorded");
        };
        assert!(matches!(&asked[..], [ContentBlock::Text { .. }, ContentBlock::Call { call, .. }] if *call == made));
        assert!(matches!(&answered[..], [ContentBlock::CallResult { output, .. }] if *output == came), "{answered:?}");
        let EventBody::ApprovalRequested { asking, description, .. } = requested else { panic!("{requested:?}") };
        assert_eq!(asking.as_deref(), Some(&Asking::ToolUse { typed: Some(ls()) }));
        assert_eq!(description.as_deref(), Some("It's 'here'"), "the rest is as it was");
        assert_eq!(serde_json::to_value(kept).unwrap(), other);
    }
    let (subagents, _) = store.list_subagents(&"s".into()).unwrap();
    let lately = subagents[0].lately.as_ref().map(|lately| &lately.calls[..]);
    assert!(matches!(lately, Some([LatelyCall { call, .. }]) if *call == ls()), "{lately:?}");
}
