//! What has to stay readable between versions. These spell the wire form out, rather than building it from
//! today's types, because changing a type would change the test along with it.

#![cfg(test)]

use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, HubStatus, ReplyOutcome,
};
use protocol::event::SessionEvent;
use protocol::frame;
use protocol::model::{RecordKey, Rev};
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

/// Spelled out for the same reason: a device sends one of these before its hello, so it has to learn of a real
/// break, or that it was revoked, from a hub of any version.
#[test]
fn what_a_device_sends_before_its_hello_keeps_its_form() {
    let pair = ClientFrame { id: 1, request: ClientRequest::Pair { code: "c".into(), device_name: "phone".into() } };
    assert_eq!(
        frame::encode(&pair).unwrap(),
        b"{\"id\":1,\"request\":{\"method\":\"pair\",\"code\":\"c\",\"device_name\":\"phone\"}}\n"
    );
    let authenticate = ClientFrame { id: 1, request: ClientRequest::Authenticate { token: "t".into() } };
    assert_eq!(
        frame::encode(&authenticate).unwrap(),
        b"{\"id\":1,\"request\":{\"method\":\"authenticate\",\"token\":\"t\"}}\n"
    );

    let reply: HubFrame =
        frame::decode(br#"{"frame":"reply","id":1,"response":{"result":"paired","device_id":"d","token":"t"}}"#)
            .unwrap();
    let paired = ClientResponse::Paired { device_id: "d".into(), token: "t".into() };
    assert_eq!(reply, HubFrame::Reply { id: 1, outcome: ReplyOutcome::Response(paired) });

    let reply: HubFrame =
        frame::decode(br#"{"frame":"reply","id":1,"error":{"code":"unauthorized","message":"no"}}"#).unwrap();
    let refused = ApiError::new(ErrorCode::Unauthorized, "no");
    assert_eq!(reply, HubFrame::Reply { id: 1, outcome: ReplyOutcome::Error(refused) });
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
