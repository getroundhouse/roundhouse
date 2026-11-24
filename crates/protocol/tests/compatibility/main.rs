//! What has to stay readable between versions. These spell the wire form out, rather than building it from
//! today's types, because changing a type would change the test along with it.

#![cfg(test)]

use std::fmt::Debug;

use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, HubStart, HubStatus,
    ReplyOutcome, records,
};
use protocol::event::{AgentTask, EventBody, SessionEvent, TaskKind};
use protocol::frame;
use protocol::model::{LimitWindow, ModelLabel, ModelName, Place, RecordKey, Rev};
use protocol::tail::{StreamDelta, Tail};
use serde::Serialize;
use serde::de::DeserializeOwned;

mod agents;
mod approvals;
mod calls;
mod capabilities;
mod change_summaries;
mod client_calls;
mod comments;
mod compactions;
mod cover_lines;
mod events;
mod every_facet;
mod git_credentials;
mod github_accounts;
mod github_reach;
mod hellos;
mod hub_updates;
mod interactions;
mod model_lists;
mod observations;
mod outputs;
mod prompts;
mod proposals;
mod provider_messages;
mod records_sent;
mod references;
mod refusals;
mod replies_heard;
mod repo_sources;
mod repos;
mod restarts;
mod session_observations;
mod sessions;
mod subagents;
mod subscribe;
mod terminal_fit;
mod terminal_rows;
mod terminal_text;
mod terminal_typed;
mod terminals;
mod waits;
mod worker;
mod worker_calls;
mod workspace_pull_requests;
mod workspaces;
mod worktrees;

/// `message` is written as `line`, and `line` reads as `message`, for the calls each protocol spells out.
fn keeps_its_form<T: Serialize + DeserializeOwned + PartialEq + Debug>(message: &T, line: &str) {
    assert_eq!(String::from_utf8(frame::encode(message).unwrap()).unwrap(), format!("{line}\n"));
    assert_eq!(&frame::decode::<T>(line.as_bytes()).unwrap(), message, "{line}");
}

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
    assert_eq!(status.start(), HubStart::ByHand, "a hub from before it said it was a service");
    assert_eq!(status.oldest_running_worker_protocol, None, "a hub from before it said");
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

    assert_eq!(frame.request, ClientRequest::RepoAdd(records::RepoAdd { path: "a".to_string() }));
}

/// What the hub records itself names no run, and nor does a delta as its worker sends it, before the hub sets it.
#[test]
fn an_event_or_delta_that_names_no_run_has_none() {
    let event: SessionEvent =
        frame::decode(br#"{"session_id":"s","seq":1,"ts_ms":1,"place":1,"body":{"type":"turn_ended","outcome":"success","result":null,"duration_ms":null,"cost_usd":null,"num_turns":null}}"#)
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
        br#"{"session_id":"s","seq":1,"ts_ms":1,"place":1,"body":{"type":"agent_limits","limits":[{"window":"seven_days","used_percent":4,"resets_at_ms":9},{"window":"thirty_days","used_percent":1}]}}"#,
    )
    .unwrap();

    let EventBody::AgentLimits { limits } = event.body else { panic!("{:?}", event.body) };
    let said: Vec<_> = limits
        .iter()
        .map(|limit| (limit.window, limit.model.clone(), limit.used_percent, limit.resets_at_ms))
        .collect();
    assert_eq!(said, [(LimitWindow::SevenDays, None, 4, Some(9)), (LimitWindow::Unknown, None, 1, None)]);
}

/// Spelled out, since an agent could hear one by itself, and the log would hold it for good.
#[test]
fn a_models_own_week_keeps_its_form_with_the_models_label() {
    let event: SessionEvent = frame::decode(
        br#"{"session_id":"s","seq":1,"ts_ms":1,"place":1,"body":{"type":"agent_limits","limits":[{"window":"model_seven_days","model":"Fable","used_percent":88,"resets_at_ms":9}]}}"#,
    )
    .unwrap();

    let EventBody::AgentLimits { limits } = event.body else { panic!("{:?}", event.body) };
    let said: Vec<_> = limits
        .iter()
        .map(|limit| (limit.window, limit.model.clone(), limit.used_percent, limit.resets_at_ms))
        .collect();
    assert_eq!(said, [(LimitWindow::ModelSevenDays, Some(ModelLabel::new("Fable")), 88, Some(9))]);
}

/// As the agent's worker sends it, before the hub adds when the turn started, and as an agent that doesn't say its
/// models or tokens ends one.
#[test]
fn a_turns_end_that_doesnt_say_its_start_models_or_tokens_has_none() {
    let event: SessionEvent =
        frame::decode(br#"{"session_id":"s","seq":1,"ts_ms":1,"place":1,"body":{"type":"turn_ended","outcome":"success","result":null,"duration_ms":9,"cost_usd":null,"num_turns":null}}"#)
            .unwrap();

    let EventBody::TurnEnded { started_at_ms, models, tokens, .. } = event.body else { panic!("{event:?}") };
    assert_eq!((started_at_ms, models, tokens), (None, Vec::new(), None));
}

/// Spelled out, since the log holds this form for good.
#[test]
fn a_turns_end_keeps_the_form_of_its_models_and_tokens() {
    let stored = br#"{"session_id":"s","seq":1,"ts_ms":9,"place":1,"body":{"type":"turn_ended","outcome":"success","result":null,"errors":[],"duration_ms":9,"cost_usd":null,"num_turns":null,"started_at_ms":1,"models":["claude-sonnet-5-5","claude-haiku-5-5"],"tokens":{"input_tokens":6,"output_tokens":550,"cache_read_tokens":1142851,"cache_write_tokens":1615}}}"#;

    let event: SessionEvent = frame::decode(stored).unwrap();

    let EventBody::TurnEnded { models, tokens, .. } = &event.body else { panic!("{event:?}") };
    let models: Vec<&str> = models.iter().map(ModelName::as_str).collect();
    assert_eq!(models, ["claude-sonnet-5-5", "claude-haiku-5-5"]);
    let tokens = tokens.unwrap();
    let counts = (tokens.input_tokens, tokens.output_tokens, tokens.cache_read_tokens, tokens.cache_write_tokens);
    assert_eq!(counts, (6, 550, 1_142_851, 1_615));
    assert_eq!(serde_json::to_vec(&event).unwrap(), stored, "and it's written the way it's read");
}

/// So a client sorting by when each workspace was last worked in puts one whose record doesn't say, as one stored
/// before the hub kept it may not, by when it was made.
#[test]
fn a_workspace_that_doesnt_say_when_it_was_last_worked_in_goes_by_when_it_was_made() {
    let line = br#"{"notice":"workspace","id":"w","repo_id":"r","name":"n","branch":"n","base_ref":null,"path":null,"status":"ready","provider":"local","instance_id":"w","created_at_ms":7,"archived_at_ms":null,"setup":{"state":"succeeded"},"port_base":55000,"rev":2}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    let HubNotice::Workspace(workspace) = notice else { panic!("{notice:?}") };
    assert_eq!((workspace.user_active_at_ms, workspace.last_active_at_ms()), (None, 7));
}

/// A hub stores an event as this and finds it by this kind, so both have to stay as they are once one is recorded.
#[test]
fn a_dropped_call_keeps_the_form_and_the_kind_it_s_stored_under() {
    let stored =
        br#"{"session_id":"s","seq":1,"ts_ms":1,"place":1,"body":{"type":"tool_call_dropped","tool_use_id":"srvtoolu_1"}}"#;

    let event: SessionEvent = frame::decode(stored).unwrap();

    assert_eq!(event.body, EventBody::ToolCallDropped { tool_use_id: "srvtoolu_1".into() });
    assert_eq!(event.body.kind(), "tool_call_dropped");
    assert_eq!(serde_json::to_value(&event.body).unwrap()["type"], "tool_call_dropped");
}

/// A list the hub stored before it kept kinds has none, which the hub reads as its record's list and sends filled in.
/// Everything left running was drawn alike then, and a command is the kind that claims least.
#[test]
fn a_background_task_stored_before_kinds_were_kept_is_a_command() {
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
    let monitor = AgentTask { id: "t1".into(), description: None, kind: TaskKind::Monitor, call: None, subagent: None };
    assert_eq!(serde_json::to_string(&monitor).unwrap(), r#"{"id":"t1","description":null,"kind":"monitor"}"#);
}

/// A list the hub stored before it kept which call started each task names none, as does a task whose agent doesn't
/// say, so a subagent's line has no button to its tab. One that names a call keeps it.
#[test]
fn a_background_task_that_names_no_call_has_none() {
    let unnamed: AgentTask = serde_json::from_str(r#"{"id":"t1","description":null,"kind":"subagent"}"#).unwrap();
    let newer = r#"{"id":"t1","description":null,"kind":"subagent","call":"toolu_1"}"#;
    let task: AgentTask = serde_json::from_str(newer).unwrap();

    assert_eq!((unnamed.call, task.call.clone()), (None, Some("toolu_1".into())));
    assert_eq!(serde_json::to_string(&task).unwrap(), newer);
}

/// A chat's log is held by place, so an event without one isn't an event of it, and doesn't read. One that names no
/// subagent is the agent's own.
#[test]
fn an_event_without_its_place_doesnt_read_and_one_that_names_no_subagent_is_the_agents_own() {
    let line = r#"{"session_id":"s","seq":1,"ts_ms":1,"place":1,"body":{"type":"agent_other","kind":"tool_progress"}}"#;

    let event: SessionEvent = frame::decode(line.as_bytes()).unwrap();

    assert_eq!((event.subagent, event.place), (None, Place::FIRST));
    assert_eq!(event.body.subagent(), None);
    assert!(frame::decode::<SessionEvent>(line.replace(r#""place":1,"#, "").as_bytes()).is_err());
}

/// A tail with no subagent in the background leaves the list out, as the hub writes it.
#[test]
fn a_tail_with_no_subagent_in_the_background_says_none_and_keeps_its_form() {
    let none = br#"{"messages":[{"message_id":"m","parent_tool_use_id":"toolu_1","blocks":[]}]}"#;
    let tail: Tail = frame::decode(none).unwrap();

    assert!(tail.in_background.is_empty());
    assert_eq!(frame::encode(&tail).unwrap(), [none.as_slice(), b"\n"].concat(), "none is left out");
}
