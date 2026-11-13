//! What has to stay readable between versions. These spell the wire form out, rather than building it from
//! today's types, because changing a type would change the test along with it.

#![cfg(test)]

use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, HubStatus, ReplyOutcome,
};
use protocol::event::{AgentTask, CompactedBy, CompactionOutcome, EventBody, SessionEvent, TaskKind};
use protocol::frame;
use protocol::model::{LimitWindow, ModelName, Place, PullRequestFound, RecordKey, Rev};
use protocol::tail::{Compacting, StreamDelta, Tail};

mod agents;
mod approvals;
mod calls;
mod events;
mod hellos;
mod prompts;
mod refusals;
mod restarts;
mod sessions;
mod worker;
mod worker_calls;
mod workspaces;

/// Spelled out, because these have to stay readable by a client of any version, even across a real
/// break. If this fails, a client can no longer see or stop a hub it can't otherwise talk to.
#[test]
fn what_works_across_a_protocol_break_keeps_its_form() {
    let status = ClientFrame { id: 1, request: ClientRequest::HubStatus };
    assert_eq!(frame::encode(&status).unwrap(), b"{\"id\":1,\"request\":{\"method\":\"hub_status\"}}\n");
    let stop = ClientFrame { id: 2, request: ClientRequest::HubShutdown };
    assert_eq!(frame::encode(&stop).unwrap(), b"{\"id\":2,\"request\":{\"method\":\"hub_shutdown\"}}\n");

    let reply: HubFrame = frame::decode(
        br#"{"frame":"reply","id":1,"response":{"result":"hub_status","pid":7,"hub_version":"a","protocol_version":3,"started_at_ms":1,"workers":4,"running_sessions":2,"clients":1,"on_demand":true}}"#,
    )
    .unwrap();
    let HubFrame::Reply { outcome: ReplyOutcome::Response(ClientResponse::HubStatus(status)), .. } = reply else {
        panic!("{reply:?}");
    };
    assert_eq!((status.pid, status.running_sessions, status.on_demand), (7, 2, true));

    let reply: HubFrame = frame::decode(br#"{"frame":"reply","id":2,"response":{"result":"ok"}}"#).unwrap();
    assert_eq!(reply, HubFrame::Reply { id: 2, outcome: ReplyOutcome::Response(ClientResponse::Ok) });

    let reply: HubFrame =
        frame::decode(br#"{"frame":"reply","id":3,"error":{"code":"version_mismatch","message":"no"}}"#).unwrap();
    let refused = ApiError::new(ErrorCode::VersionMismatch, "no");
    assert_eq!(reply, HubFrame::Reply { id: 3, outcome: ReplyOutcome::Error(refused) });
}

#[test]
fn a_status_from_a_hub_older_than_on_demand_reads_as_not_on_demand() {
    let status: HubStatus = frame::decode(
        br#"{"pid":7,"hub_version":"a","protocol_version":3,"started_at_ms":1,"workers":0,"running_sessions":0,"clients":1}"#,
    )
    .unwrap();

    assert!(!status.on_demand);
}

#[test]
fn a_request_from_a_newer_client_still_decodes_with_its_id() {
    let line = br#"{"id":7,"request":{"method":"from_a_newer_client","path":"a","count":2}}"#;

    let frame: ClientFrame = frame::decode(line).unwrap();

    assert_eq!(frame, ClientFrame { id: 7, request: ClientRequest::Unknown });
}

#[test]
fn a_known_request_with_a_new_field_still_decodes() {
    let line = br#"{"id":7,"request":{"method":"repo_add","path":"a","from_a_newer_client":true}}"#;

    let frame: ClientFrame = frame::decode(line).unwrap();

    assert_eq!(frame.request, ClientRequest::RepoAdd { path: "a".to_string() });
}

#[test]
fn a_record_from_a_hub_without_revisions_reads_as_revision_zero() {
    let line = br#"{"notice":"repo","id":"r","name":"repo","path":"/repo","created_at_ms":1}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    let HubNotice::Repo(repo) = notice else { panic!("{notice:?}") };
    assert_eq!(repo.rev, Rev::default());
}

#[test]
fn an_event_or_delta_from_a_hub_without_runs_has_none() {
    let event: SessionEvent =
        frame::decode(br#"{"session_id":"s","seq":1,"ts_ms":1,"body":{"type":"turn_ended","outcome":"success","result":null,"duration_ms":null,"cost_usd":null,"num_turns":null}}"#)
            .unwrap();
    let delta: StreamDelta =
        frame::decode(br#"{"message_id":"m","change":{"type":"text","index":0,"text":"hi"}}"#).unwrap();

    assert_eq!((event.run, delta.run), (None, None));
}

#[test]
fn a_removed_record_of_a_kind_this_build_does_not_know_still_reads() {
    let line = br#"{"notice":"removed","record":{"kind":"pin","id":"p"},"rev":3}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    assert_eq!(notice, HubNotice::Removed { record: RecordKey::Unknown, rev: Rev::new(3) });
}

/// Spelled out, since the log holds this form for good.
#[test]
fn what_an_agent_heard_of_its_accounts_limits_keeps_its_form_and_a_newer_window_still_reads() {
    let event: SessionEvent = frame::decode(
        br#"{"session_id":"s","seq":1,"ts_ms":1,"body":{"type":"agent_limits","limits":[{"window":"seven_days","used_percent":4,"resets_at_ms":9},{"window":"thirty_days","used_percent":1}]}}"#,
    )
    .unwrap();

    let EventBody::AgentLimits { limits } = event.body else { panic!("{:?}", event.body) };
    let said: Vec<_> = limits.iter().map(|limit| (limit.window, limit.used_percent, limit.resets_at_ms)).collect();
    assert_eq!(said, [(LimitWindow::SevenDays, 4, Some(9)), (LimitWindow::Unknown, 1, None)]);
}

#[test]
fn a_turns_end_from_a_hub_that_doesnt_add_its_start_or_keep_its_models_and_tokens_has_none() {
    let event: SessionEvent =
        frame::decode(br#"{"session_id":"s","seq":1,"ts_ms":1,"body":{"type":"turn_ended","outcome":"success","result":null,"duration_ms":9,"cost_usd":null,"num_turns":null}}"#)
            .unwrap();

    let EventBody::TurnEnded { started_at_ms, models, tokens, .. } = event.body else { panic!("{event:?}") };
    assert_eq!((started_at_ms, models, tokens), (None, Vec::new(), None));
}

/// Spelled out, since the log holds this form for good.
#[test]
fn a_turns_end_keeps_the_form_of_its_models_and_tokens() {
    let stored = br#"{"session_id":"s","seq":1,"ts_ms":9,"place":0,"body":{"type":"turn_ended","outcome":"success","result":null,"errors":[],"duration_ms":9,"cost_usd":null,"num_turns":null,"started_at_ms":1,"models":["claude-sonnet-5-5","claude-haiku-5-5"],"tokens":{"input_tokens":6,"output_tokens":550,"cache_read_tokens":1142851,"cache_write_tokens":1615}}}"#;

    let event: SessionEvent = frame::decode(stored).unwrap();

    let EventBody::TurnEnded { models, tokens, .. } = &event.body else { panic!("{event:?}") };
    let models: Vec<&str> = models.iter().map(ModelName::as_str).collect();
    assert_eq!(models, ["claude-sonnet-5-5", "claude-haiku-5-5"]);
    let tokens = tokens.unwrap();
    let counts = (tokens.input_tokens, tokens.output_tokens, tokens.cache_read_tokens, tokens.cache_write_tokens);
    assert_eq!(counts, (6, 550, 1_142_851, 1_615));
    assert_eq!(serde_json::to_vec(&event).unwrap(), stored, "and it's written the way it's read");
}

/// An approval closed by a hub from before it kept answers, or closed any other way, has none, and is written as it was.
#[test]
fn an_approval_closed_without_answers_has_none_and_keeps_its_form() {
    let stored = br#"{"session_id":"s","seq":1,"ts_ms":1,"place":0,"body":{"type":"approval_resolved","approval_id":"a","outcome":"allowed"}}"#;

    let event: SessionEvent = frame::decode(stored).unwrap();

    let EventBody::ApprovalResolved { answers_by_text, answers_by_position, .. } = &event.body else {
        panic!("{event:?}")
    };
    assert_eq!((answers_by_text, answers_by_position), (&None, &None));
    assert_eq!(serde_json::to_vec(&event).unwrap(), stored, "and it's written the way it's read");
}

/// Spelled out, since the log holds this form for good.
#[test]
fn an_approval_closed_with_answers_keeps_their_form() {
    let stored = br#"{"session_id":"s","seq":1,"ts_ms":9,"place":0,"body":{"type":"approval_resolved","approval_id":"a","outcome":"allowed","answers":{"Red or blue?":"Blue","Which days?":"Monday, Friday"}}}"#;

    let event: SessionEvent = frame::decode(stored).unwrap();

    let EventBody::ApprovalResolved { answers_by_text: Some(answers), .. } = &event.body else { panic!("{event:?}") };
    let answers: Vec<(&str, &str)> = answers.iter().map(|(q, a)| (q.as_str(), a.as_str())).collect();
    assert_eq!(answers, [("Red or blue?", "Blue"), ("Which days?", "Monday, Friday")]);
    assert_eq!(serde_json::to_vec(&event).unwrap(), stored, "and it's written the way it's read");
}

/// So a client tells a hub that hasn't looked for a workspace's pull request, which it waits for, from one that looked
/// and found none, which it can offer to create.
#[test]
fn a_workspace_tells_a_pull_request_not_looked_for_from_none() {
    let workspace = r#"{"notice":"workspace","id":"w","repo_id":"r","name":"n","branch":"n","base_ref":null,"path":null,"status":"ready","provider":"local","instance_id":"w","created_at_ms":1,"archived_at_ms":null,"setup":{"state":"succeeded"},"port_base":55000"#;
    let read = |rest: &str| {
        let notice: HubNotice = frame::decode(format!("{workspace}{rest}}}").as_bytes()).unwrap();
        let HubNotice::Workspace(workspace) = notice else { panic!("{notice:?}") };
        workspace.pull_request
    };

    let not_looked = read("");
    let none = read(r#","pull_request":null"#);
    let found = read(r#","pull_request":{"number":7,"url":"u","title":"T","state":"OPEN","is_draft":false}"#);

    assert_eq!(not_looked, None);
    assert_eq!(none, Some(PullRequestFound { pull_request: None }));
    assert_eq!(found.and_then(|found| found.pull_request).map(|pr| pr.number), Some(7));
}

/// So a client sorting by when each workspace was last worked in puts one from a hub that doesn't keep it by when it was
/// made.
#[test]
fn a_workspace_from_a_hub_that_doesnt_keep_when_it_was_last_worked_in_goes_by_when_it_was_made() {
    let line = br#"{"notice":"workspace","id":"w","repo_id":"r","name":"n","branch":"n","base_ref":null,"path":null,"status":"ready","provider":"local","instance_id":"w","created_at_ms":7,"archived_at_ms":null,"setup":{"state":"succeeded"},"port_base":55000}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    let HubNotice::Workspace(workspace) = notice else { panic!("{notice:?}") };
    assert_eq!((workspace.user_active_at_ms, workspace.last_active_at_ms()), (None, 7));
}

/// A hub stores an event as this and finds it by this kind, so both have to stay as they are once one is recorded.
#[test]
fn a_dropped_call_keeps_the_form_and_the_kind_it_s_stored_under() {
    let stored =
        br#"{"session_id":"s","seq":1,"ts_ms":1,"body":{"type":"tool_call_dropped","tool_use_id":"srvtoolu_1"}}"#;

    let event: SessionEvent = frame::decode(stored).unwrap();

    assert_eq!(event.body, EventBody::ToolCallDropped { tool_use_id: "srvtoolu_1".into() });
    assert_eq!(event.body.kind(), "tool_call_dropped");
    assert_eq!(serde_json::to_value(&event.body).unwrap()["type"], "tool_call_dropped");
}

/// A list the hub stored before it kept kinds, or sent by a hub that doesn't, has none. Everything left running was
/// drawn alike then, and a command is the kind that claims least.
#[test]
fn a_background_task_from_a_hub_that_doesnt_keep_kinds_is_a_command() {
    let task: AgentTask = serde_json::from_str(r#"{"id":"t1","description":"Run the tests"}"#).unwrap();

    assert_eq!(task.kind, TaskKind::Command);
}

/// A newer hub's kind mustn't cost the list: the hub reads a stored list it can't parse as empty, which would say
/// nothing is running.
#[test]
fn a_background_task_of_a_kind_this_build_doesnt_know_is_a_command() {
    let kind = |kind: &str| -> TaskKind {
        let task: AgentTask = serde_json::from_str(&format!(r#"{{"id":"t1","kind":"{kind}"}}"#)).unwrap();
        task.kind
    };

    assert_eq!(kind("something_newer"), TaskKind::Command);
    assert_eq!(
        (kind("subagent"), kind("monitor"), kind("wait"), kind("command")),
        (TaskKind::Subagent, TaskKind::Monitor, TaskKind::Wait, TaskKind::Command)
    );
    let monitor = AgentTask { id: "t1".into(), description: None, kind: TaskKind::Monitor, call: None };
    assert_eq!(serde_json::to_string(&monitor).unwrap(), r#"{"id":"t1","description":null,"kind":"monitor"}"#);
}

/// A list the hub stored before it kept which call started each task, or sent by a hub that doesn't, names none, so
/// a subagent's line has no button to its tab. One that names a call keeps it.
#[test]
fn a_background_task_from_a_hub_that_doesnt_keep_its_call_has_none() {
    let older: AgentTask = serde_json::from_str(r#"{"id":"t1","description":null,"kind":"subagent"}"#).unwrap();
    let newer = r#"{"id":"t1","description":null,"kind":"subagent","call":"toolu_1"}"#;
    let task: AgentTask = serde_json::from_str(newer).unwrap();

    assert_eq!((older.call, task.call.clone()), (None, Some("toolu_1".into())));
    assert_eq!(serde_json::to_string(&task).unwrap(), newer);
}

#[test]
fn an_event_from_a_hub_that_doesnt_record_subagents_is_the_agents_own_at_place_zero() {
    let event: SessionEvent =
        frame::decode(br#"{"session_id":"s","seq":1,"ts_ms":1,"body":{"type":"agent_other","kind":"tool_progress"}}"#)
            .unwrap();

    assert_eq!((event.subagent, event.place), (None, Place::default()));
    assert_eq!(event.body.subagent(), None);
}

#[test]
fn a_tail_from_a_hub_that_doesnt_say_which_subagents_are_in_the_background_has_none_and_keeps_its_form() {
    let older = br#"{"messages":[{"message_id":"m","parent_tool_use_id":"toolu_1","blocks":[]}]}"#;
    let tail: Tail = frame::decode(older).unwrap();

    assert!(tail.in_background.is_empty());
    assert_eq!(frame::encode(&tail).unwrap(), [older.as_slice(), b"\n"].concat(), "none is left out");
}

/// A hub stores a compaction, and what asked for one, as these and finds them by these kinds, so both have to stay as
/// they are once one is recorded.
#[test]
fn a_compaction_and_what_asked_for_it_keep_their_form_and_the_kinds_they_re_stored_under() {
    let asked: &[u8] = br#"{"type":"compaction_asked","keeping":"The API design"}"#;
    let compacting: &[u8] = br#"{"type":"agent_compacting","written_chars":8463}"#;
    let compacted: &[u8] = br#"{"type":"agent_compacted","asked":7,"keeping":"The API design","outcome":{"result":"compacted","by":"asked","before_tokens":75284,"after_tokens":15050,"took_ms":31747,"summary":"1. What was asked"}}"#;
    let read = |body: &[u8]| -> EventBody { serde_json::from_slice(body).unwrap() };

    let outcome = CompactionOutcome::Compacted {
        by: CompactedBy::Asked,
        before_tokens: Some(75_284),
        after_tokens: Some(15_050),
        took_ms: Some(31_747),
        summary: Some("1. What was asked".into()),
    };
    assert_eq!(read(asked), EventBody::CompactionAsked { keeping: Some("The API design".into()) });
    assert_eq!(read(compacting), EventBody::AgentCompacting { written_chars: Some(8_463) });
    assert_eq!(
        read(compacted),
        EventBody::AgentCompacted { asked: Some(7), keeping: Some("The API design".into()), outcome }
    );
    for (body, kind) in [(asked, "compaction_asked"), (compacting, "agent_compacting"), (compacted, "agent_compacted")]
    {
        assert_eq!(read(body).kind(), kind);
        assert_eq!(serde_json::to_vec(&read(body)).unwrap(), body, "{kind} keeps its form");
    }
}

/// A failure says only why, and a compaction, or an outcome, this build doesn't know still reads.
#[test]
fn a_compaction_that_failed_or_that_this_build_doesnt_know_still_reads() {
    let read = |body: &str| -> CompactionOutcome {
        let EventBody::AgentCompacted { outcome, .. } = serde_json::from_str(body).unwrap() else { panic!("{body}") };
        outcome
    };

    let failed = read(r#"{"type":"agent_compacted","outcome":{"result":"failed","error":"Not enough messages"}}"#);
    let newer = read(r#"{"type":"agent_compacted","outcome":{"result":"postponed"}}"#);
    let by_newer = read(r#"{"type":"agent_compacted","outcome":{"result":"compacted","by":"by_schedule"}}"#);
    let by_nobody = read(r#"{"type":"agent_compacted","outcome":{"result":"compacted"}}"#);

    assert_eq!(failed, CompactionOutcome::Failed { error: Some("Not enough messages".into()) });
    assert_eq!(
        (newer, read(r#"{"type":"agent_compacted"}"#)),
        (CompactionOutcome::Unknown, CompactionOutcome::Unknown)
    );
    for unknown in [by_newer, by_nobody] {
        assert!(matches!(unknown, CompactionOutcome::Compacted { by: CompactedBy::Unknown, .. }), "{unknown:?}");
    }
}

/// An older hub refuses it as a request it doesn't know, which a client tells apart.
#[test]
fn asking_to_compact_keeps_its_form() {
    let id = "s".into();
    let asked = ClientFrame { id: 4, request: ClientRequest::SessionCompact { id, keeping: Some("The API".into()) } };

    let sent = frame::encode(&asked).unwrap();

    assert_eq!(sent, b"{\"id\":4,\"request\":{\"method\":\"session_compact\",\"id\":\"s\",\"keeping\":\"The API\"}}\n");
}

/// A turn's end from an agent that doesn't say it was only a compaction wasn't, and one that wasn't says nothing of it.
#[test]
fn a_turns_end_says_it_was_only_a_compaction_only_when_it_was() {
    let only: EventBody =
        serde_json::from_str(r#"{"type":"turn_ended","outcome":"success","result":"","only_compacted":true}"#).unwrap();
    let older: EventBody = serde_json::from_str(r#"{"type":"turn_ended","outcome":"success","result":null}"#).unwrap();

    assert!(matches!(only, EventBody::TurnEnded { only_compacted: true, .. }));
    let EventBody::TurnEnded { only_compacted, .. } = &older else { panic!() };
    assert!(!only_compacted);
    assert!(!serde_json::to_string(&older).unwrap().contains("only_compacted"));
}

#[test]
fn a_tail_from_a_hub_that_doesnt_say_its_compacting_isnt_and_one_that_does_keeps_its_form() {
    let older: Tail = frame::decode(br#"{"messages":[]}"#).unwrap();
    let compacting = br#"{"messages":[],"compacting":{"run":3,"written_chars":8463}}"#;

    let tail: Tail = frame::decode(compacting).unwrap();

    assert_eq!(older.compacting, None);
    assert_eq!(tail.compacting.as_ref().and_then(|c| c.written_chars), Some(8_463));
    assert_eq!(frame::encode(&tail).unwrap(), [compacting.as_slice(), b"\n"].concat());
    assert_eq!(serde_json::from_str::<Compacting>("{}").unwrap(), Compacting::default(), "one that says nothing more");
}
