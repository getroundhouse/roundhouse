//! Records from before the hub kept them, or kept all they hold now, filled in from the log as it would have kept
//! them.

use std::collections::HashMap;

use protocol::model::SubagentStatus::{Failed, Finished, Stopped};
use serde_json::{Value, json};

use super::*;
use crate::store::{older, schema};
/// A line of the log as a build from before subagents had records wrote it, at the time `ts`.
struct Line {
    ts: i64,
    body: EventBody,
    raw: Option<String>,
}

/// An event the hub made, or the agent's that its line has no more to say about.
fn at(ts: i64, body: EventBody) -> Line {
    Line { ts, body, raw: None }
}

/// The CLI's word that a task started, as the agent's own event the build that recorded it made of it.
fn task_started(ts: i64, task: &str, task_type: &str, call: &Value) -> Line {
    let raw = json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":call,"task_type":task_type,
        "subagent_type":"Explore","description":"Find the tests","prompt":"Find where the tests live.",
        "is_backgrounded":task == "t1"});
    Line { ts, body: other("system/task_started"), raw: Some(raw.to_string()) }
}

fn task_notification(ts: i64, task: &str, status: &str) -> Line {
    let raw = json!({"type":"system","subtype":"task_notification","task_id":task,"status":status,"summary":"Found."});
    Line { ts, body: other("system/task_notification"), raw: Some(raw.to_string()) }
}

fn rows(lines: &[Line]) -> String {
    let mut places: HashMap<Option<ToolUseId>, u64> = HashMap::new();
    let session = format!(
        "INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms, updated_at_ms) \
         VALUES ('s', 'w', 'claude', 's', 'idle', {}, 0, 0);",
        lines.len()
    );
    let events = (1..).zip(lines).map(|(seq, Line { ts, body, raw })| {
        let subagent = body.subagent();
        let place = places.entry(subagent.cloned()).or_default();
        *place += 1;
        let quoted = |text: Option<&str>| text.map_or_else(|| "NULL".to_owned(), |text| format!("'{text}'"));
        format!(
            "INSERT INTO events (session_id, seq, ts_ms, kind, subagent, place, body, raw) \
             VALUES ('s', {seq}, {ts}, '{}', {}, {place}, '{}', {});",
            body.kind(),
            quoted(subagent.map(ToolUseId::as_str)),
            serde_json::to_string(body).unwrap(),
            quoted(raw.as_deref()),
        )
    });
    std::iter::once(session).chain(events).collect()
}

#[test]
fn a_migrated_log_fills_in_each_subagents_record_as_the_hub_would_have_kept_it() {
    let dir = tempfile::tempdir().unwrap();
    let grep =
        ContentBlock::ToolUse { id: "grep".into(), name: "Grep".into(), input: json!({"pattern": "test"}), call: None };
    let mut failed_start = task_started(14, "t2", "local_agent", &json!("failed"));
    failed_start.body = started("failed");
    let mut failed_end = task_notification(15, "t2", "failed");
    failed_end.body = ended("failed", SubagentOutcome::Failed, None);
    let lines = [
        at(9, message(Some("finished"), vec![tool_use("before")])),
        task_started(10, "t1", "local_agent", &json!("finished")),
        at(11, message(Some("finished"), vec![tool_use("read-1"), tool_use("read-2")])),
        at(12, message(None, vec![tool_use("own")])),
        at(12, message(Some("finished"), vec![grep])),
        task_notification(13, "t1", "completed"),
        // As the build that read the CLI's word on a subagent's task recorded it.
        failed_start,
        failed_end,
        task_started(16, "t3", "local_bash", &json!("bash")),
        task_started(17, "t4", "local_agent", &json!("exited")),
        at(18, exited()),
        task_notification(19, "t4", "completed"),
        task_started(20, "t5", "local_agent", &json!("never")),
        Line { ts: 21, raw: Some("Warning: not JSON".into()), ..at(21, other("x")) },
        task_started(22, "t6", "local_agent", &json!(7)),
        task_started(23, "t7", "local_agent", &json!("odd")),
        task_notification(24, "t7", "paused"),
    ];

    let store = older::store_from_before(dir.path(), schema::BEFORE_SUBAGENT_RECORDS, &rows(&lines));

    let (subagents, rev) = store.list_subagents(&"s".into()).unwrap();
    let kept: Vec<_> = subagents
        .iter()
        .map(|s| {
            let latest = s.latest_call.as_ref().map(|call| call.id.as_str());
            (s.call.as_str(), s.status, s.started_at_ms, s.ended_at_ms, s.calls, latest, s.report.as_deref())
        })
        .collect();
    assert_eq!(
        kept,
        [
            ("finished", Finished, 10, Some(13), 3, Some("grep"), Some("Found.")),
            ("failed", Failed, 14, Some(15), 0, None, None),
            ("exited", Stopped, 17, Some(18), 0, None, None),
            // It went with its agent by the session's last event.
            ("never", Stopped, 20, Some(24), 0, None, None),
            ("odd", SubagentStatus::Unknown, 23, Some(24), 0, None, None),
        ]
    );
    let finished = &subagents[0];
    assert_eq!((finished.kind.as_deref(), finished.description.as_deref()), (Some("Explore"), Some("Find the tests")));
    assert_eq!((finished.prompt.as_deref(), finished.in_background), (Some("Find where the tests live."), true));
    let grep = Call { id: "grep".into(), name: "Grep".into(), input: json!({"pattern": "test"}), kind: None };
    assert_eq!(finished.latest_call.as_ref(), Some(&grep));
    assert!(!subagents[1].in_background);
    assert!(subagents.iter().all(|s| s.rev > Rev::default() && s.rev <= rev));

    assert_eq!(record(&store, &ended("never", SubagentOutcome::Finished, Some("Late."))).1, Vec::<String>::new());
    assert_eq!(record(&store, &started("new")).1, ["new"]);
    assert!(subagent(&store, "new").rev > rev, "its revisions go on from the counter's");
}

/// JavaScript can write half of a surrogate pair, such as half of an emoji. That isn't JSON to the adapter, which
/// repairs the line first, but SQLite reads it, into bytes that aren't UTF-8.
#[test]
fn half_a_surrogate_pair_in_a_migrated_line_reads_as_the_adapter_repairs_it() {
    let dir = tempfile::tempdir().unwrap();
    let summary = r"Found 한 \ud83d\ude00 and \ud83d";
    let raw = json!({"type":"system","subtype":"task_notification","task_id":"t1","status":"completed"});
    let raw = raw.to_string().replace('}', &format!(r#","summary":"{summary}"}}"#));
    let lines = [
        task_started(1, "t1", "local_agent", &json!("call")),
        Line { raw: Some(raw), ..task_notification(2, "t1", "completed") },
    ];

    let store = older::store_from_before(dir.path(), schema::BEFORE_SUBAGENT_RECORDS, &rows(&lines));

    let (subagents, _) = store.list_subagents(&"s".into()).unwrap();
    assert_eq!(subagents[0].report.as_deref(), Some("Found 한 😀 and \u{fffd}"));
    assert_eq!(subagent(&store, "call").report, subagents[0].report);
}

/// A record from before has done nothing lately that it knows of, and fills in as it goes on.
#[test]
fn a_migrated_record_starts_with_nothing_done_lately() {
    let dir = tempfile::tempdir().unwrap();
    let old = "INSERT INTO subagents (session_id, call, status, started_at_ms) VALUES ('s', 'old', 'working', 1);";
    let rows = format!("{}{old}", rows(&[]));

    let store = older::store_from_before(dir.path(), schema::BEFORE_RECENT_WORK, &rows);

    assert_eq!(subagent(&store, "old").recent, RecentWork::default());
    record(&store, &message(Some("old"), vec![tool_use("read")]));
    assert_eq!(recent(&store, "old"), (None, vec![("read".into(), None)]));
}

/// A record from before takes its model from the newest of its replies in the log that names one, and one without
/// any keeps its revision.
#[test]
fn a_migrated_record_names_the_model_of_its_newest_reply() {
    let dir = tempfile::tempdir().unwrap();
    let thinking = || vec![ContentBlock::Thinking { text: "x".into() }];
    let lines = [
        at(1, reply(Some("haiku"), "claude-opus-5-5", thinking())),
        at(2, reply(Some("haiku"), "claude-haiku-5-5", thinking())),
        at(3, reply(None, "claude-fable-5-2", thinking())),
        // As the CLI names a message it wrote itself, such as one saying the API failed.
        at(4, reply(Some("haiku"), "<synthetic>", thinking())),
        at(5, answer(Some("haiku"), "read", "x")),
        at(6, message(Some("haiku"), thinking())),
    ];
    // In this order, so a migration that changed the quiet one too would leave it the newer.
    let records = "INSERT INTO subagents (session_id, call, status, started_at_ms) VALUES ('s', 'haiku', 'working', 1);
        INSERT INTO subagents (session_id, call, status, started_at_ms) VALUES ('s', 'quiet', 'working', 1);";

    let store =
        older::store_from_before(dir.path(), schema::BEFORE_SUBAGENT_MODELS, &format!("{}{records}", rows(&lines)));

    assert_eq!(model(&store, "haiku").as_deref(), Some("claude-haiku-5-5"));
    assert_eq!(model(&store, "quiet"), None);
    assert!(subagent(&store, "quiet").rev < subagent(&store, "haiku").rev, "only one with a model changed");
}
