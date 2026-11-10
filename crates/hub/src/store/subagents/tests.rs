use std::collections::HashMap;

use protocol::event::{ExitReason, SubagentOutcome};
use protocol::model::SubagentStatus::{Failed, Finished, Stopped, Working};
use serde_json::{Value, json};

use super::*;
use crate::store::tests::store_with_session;
use crate::store::{older, schema};

fn started(call: &str) -> EventBody {
    EventBody::SubagentStarted {
        call: call.into(),
        kind: Some("Explore".into()),
        description: Some("Find the tests".into()),
        prompt: Some("Find where the tests live.".into()),
        in_background: false,
    }
}

fn ended(call: &str, outcome: SubagentOutcome, report: Option<&str>) -> EventBody {
    EventBody::SubagentEnded { call: call.into(), outcome, report: report.map(Into::into) }
}

fn tool_use(id: &str) -> ContentBlock {
    ContentBlock::ToolUse { id: id.into(), name: "Read".into(), input: json!({"file_path": id}) }
}

/// A message from the agent, or from the subagent `from` started.
fn message(from: Option<&str>, blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant {
        message_id: None,
        parent_tool_use_id: from.map(Into::into),
        model: None,
        blocks,
        first_index: None,
    }
}

fn other(kind: &str) -> EventBody {
    EventBody::AgentOther { kind: kind.into(), parent_tool_use_id: None }
}

fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::HubRestart, code: None, signal: None, stderr_tail: vec![] }
}

/// Records `body` in the session `s`, and says which subagents' records it changed.
fn record(store: &Store, body: &EventBody) -> (SessionEvent, Vec<String>) {
    let appended = store.append(&"s".into(), None, body, None, None, None).unwrap();
    (appended.event, appended.subagents.iter().map(|call| call.as_str().to_owned()).collect())
}

fn subagent(store: &Store, call: &str) -> Subagent {
    store.get_subagent(&"s".into(), &call.into()).unwrap().unwrap()
}

#[test]
fn a_subagents_record_starts_with_what_it_was_asked_counts_its_calls_and_ends_with_its_report() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let (start, changed) = record(&store, &started("agent"));
    assert_eq!(changed, ["agent"]);
    let first = subagent(&store, "agent");
    assert_eq!((first.status, first.started_at_ms, first.calls, &first.latest_call), (Working, start.ts_ms, 0, &None));
    assert_eq!((first.kind.as_deref(), first.prompt.as_deref()), (Some("Explore"), Some("Find where the tests live.")));

    assert_eq!(record(&store, &message(None, vec![tool_use("own")])).1, Vec::<String>::new(), "the agent's own");
    assert_eq!(record(&store, &message(Some("other"), vec![tool_use("x")])).1, Vec::<String>::new(), "no record");
    let text = vec![ContentBlock::Text { text: "x".into() }];
    assert_eq!(record(&store, &message(Some("agent"), text)).1, Vec::<String>::new(), "no call");
    let blocks = vec![tool_use("read-1"), ContentBlock::Text { text: "x".into() }, tool_use("read-2")];
    assert_eq!(record(&store, &message(Some("agent"), blocks)).1, ["agent"]);
    let working = subagent(&store, "agent");
    assert_eq!(working.calls, 2);
    assert_eq!(working.latest_call.as_ref().map(|call| call.id.as_str()), Some("read-2"), "the newest");
    assert_eq!(working.latest_call.unwrap().input, json!({"file_path": "read-2"}));
    assert!(working.rev > first.rev, "a client only takes a newer copy");

    let (end, changed) = record(&store, &ended("agent", SubagentOutcome::Finished, Some("They're in tests/.")));
    assert_eq!(changed, ["agent"]);
    let finished = subagent(&store, "agent");
    assert_eq!((finished.status, finished.ended_at_ms), (Finished, Some(end.ts_ms)));
    assert_eq!((finished.calls, finished.report.as_deref()), (2, Some("They're in tests/.")));
    assert!(finished.rev > working.rev);

    assert_eq!(record(&store, &ended("agent", SubagentOutcome::Failed, None)).1, Vec::<String>::new(), "ended once");
    assert_eq!(record(&store, &started("agent")).1, Vec::<String>::new(), "started once");
    assert_eq!(subagent(&store, "agent"), finished);
}

#[test]
fn an_agent_that_exits_takes_the_subagents_still_working_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for body in [started("working"), started("done"), ended("done", SubagentOutcome::Finished, Some("Done."))] {
        record(&store, &body);
    }
    let done = subagent(&store, "done");

    let (exit, changed) = record(&store, &exited());

    assert_eq!(changed, ["working"]);
    let stopped = subagent(&store, "working");
    assert_eq!((stopped.status, stopped.ended_at_ms, stopped.report), (Stopped, Some(exit.ts_ms), None));
    assert_eq!(subagent(&store, "done"), done, "it had ended already");
    assert_eq!(record(&store, &exited()).1, Vec::<String>::new());
}

#[test]
fn a_sessions_subagents_are_listed_oldest_first_as_of_a_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut other = store.get_session(&"s".into()).unwrap();
    other.session.id = "t".into();
    store.insert_session(&other, None).unwrap();
    record(&store, &started("b"));
    record(&store, &started("a"));
    store.append(&"t".into(), None, &started("c"), None, None, None).unwrap();

    let (subagents, rev) = store.list_subagents(&"s".into()).unwrap();

    let calls: Vec<&str> = subagents.iter().map(|subagent| subagent.call.as_str()).collect();
    assert_eq!(calls, ["b", "a"], "in the order they started, not by name");
    assert!(subagents.iter().all(|subagent| subagent.rev <= rev));
    assert_eq!(store.list_subagents(&"t".into()).unwrap().0.len(), 1);

    record(&store, &ended("a", SubagentOutcome::Finished, None));
    assert!(subagent(&store, "a").rev > rev, "a change since is newer than the list");
}

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
    let grep = ContentBlock::ToolUse { id: "grep".into(), name: "Grep".into(), input: json!({"pattern": "test"}) };
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
            ("never", Stopped, 20, None, 0, None, None),
            ("odd", SubagentStatus::Unknown, 23, Some(24), 0, None, None),
        ]
    );
    let finished = &subagents[0];
    assert_eq!((finished.kind.as_deref(), finished.description.as_deref()), (Some("Explore"), Some("Find the tests")));
    assert_eq!((finished.prompt.as_deref(), finished.in_background), (Some("Find where the tests live."), true));
    let grep = Call { id: "grep".into(), name: "Grep".into(), input: json!({"pattern": "test"}) };
    assert_eq!(finished.latest_call.as_ref(), Some(&grep));
    assert!(!subagents[1].in_background);
    assert!(subagents.iter().all(|s| s.rev > Rev::default() && s.rev <= rev));

    assert_eq!(record(&store, &ended("never", SubagentOutcome::Finished, Some("Late."))).1, Vec::<String>::new());
    assert_eq!(record(&store, &started("new")).1, ["new"]);
    assert!(subagent(&store, "new").rev > rev, "its revisions go on from the counter's");
}
