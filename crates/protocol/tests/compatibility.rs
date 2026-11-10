//! What has to stay readable between versions. These spell the wire form out, rather than building it from
//! today's types, because changing a type would change the test along with it.

#![cfg(test)]

use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, HubStatus, ReplyOutcome,
};
use protocol::event::{EventBody, SessionEvent};
use protocol::frame;
use protocol::model::{LimitWindow, PullRequestFound, RecordKey, Rev};
use protocol::tail::StreamDelta;

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

#[test]
fn a_session_from_a_hub_that_doesnt_keep_replies_reads_as_read() {
    let line = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":null,"permission_mode":null,"status":"idle","last_seq":9,"created_at_ms":1,"updated_at_ms":2}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    let HubNotice::Session(session) = notice else { panic!("{notice:?}") };
    assert_eq!((session.reply_seq, session.read_seq), (0, 0));
    assert!(!session.unread());
}

#[test]
fn a_running_session_from_a_hub_that_doesnt_keep_turn_starts_has_none() {
    let line = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":null,"permission_mode":null,"status":"running","last_seq":9,"created_at_ms":1,"updated_at_ms":2}"#;

    let notice: HubNotice = frame::decode(line).unwrap();

    let HubNotice::Session(session) = notice else { panic!("{notice:?}") };
    assert_eq!(session.turn_started_at_ms, None);
}

#[test]
fn a_session_from_a_hub_that_doesnt_keep_how_full_the_context_is_has_none_and_its_form_stays_readable() {
    let older = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":null,"permission_mode":null,"status":"idle","last_seq":9,"created_at_ms":1,"updated_at_ms":2}"#;
    let HubNotice::Session(session) = frame::decode::<HubNotice>(older).unwrap() else { panic!() };
    assert_eq!(session.context, None);

    // Spelled out, since a hub's stored rows hold this form too.
    let said = br#"{"notice":"session","id":"s","workspace_id":"w","agent":"claude","model":null,"permission_mode":null,"status":"idle","last_seq":9,"created_at_ms":1,"updated_at_ms":2,"context":{"used_tokens":5,"max_tokens":10,"parts":[{"name":"Messages","tokens":5}]}}"#;
    let HubNotice::Session(session) = frame::decode::<HubNotice>(said).unwrap() else { panic!() };
    let context = session.context.unwrap();
    assert_eq!((context.used_tokens, context.max_tokens, context.parts.len()), (5, 10, 1));
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
fn a_turns_end_from_a_hub_that_doesnt_add_its_start_has_none() {
    let event: SessionEvent =
        frame::decode(br#"{"session_id":"s","seq":1,"ts_ms":1,"body":{"type":"turn_ended","outcome":"success","result":null,"duration_ms":9,"cost_usd":null,"num_turns":null}}"#)
            .unwrap();

    let EventBody::TurnEnded { started_at_ms, .. } = event.body else { panic!("{event:?}") };
    assert_eq!(started_at_ms, None);
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
