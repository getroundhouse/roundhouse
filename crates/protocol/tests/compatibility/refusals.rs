//! How a hub and a worker refuse a request from a newer peer, which has to read the same in every version, since it's
//! how the newer one learns to do without.

use protocol::client::{ApiError, ClientRequest, ErrorCode, HubFrame, ReplyOutcome};
use protocol::frame;
use protocol::worker::{WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest};

#[test]
fn a_refusal_keeps_its_form() {
    let unsupported = ApiError::new(ErrorCode::Unsupported, "no");
    let from_a_hub: HubFrame =
        frame::decode(br#"{"frame":"reply","id":4,"error":{"code":"unsupported","message":"no"}}"#).unwrap();
    assert_eq!(from_a_hub, HubFrame::Reply { id: 4, outcome: ReplyOutcome::Error(unsupported.clone()) });

    let from_a_worker = WorkerFrame::Reply { id: 4, outcome: WorkerOutcome::Error(unsupported) };
    assert_eq!(
        frame::encode(&from_a_worker).unwrap(),
        b"{\"frame\":\"reply\",\"id\":4,\"error\":{\"code\":\"unsupported\",\"message\":\"no\"}}\n"
    );
}

#[test]
fn a_request_from_a_newer_hub_still_decodes_with_its_id() {
    let line = br#"{"id":7,"request":{"method":"from_a_newer_hub","path":"a","count":2}}"#;

    let call: WorkerCall = frame::decode(line).unwrap();

    assert_eq!(call, WorkerCall { id: 7, request: WorkerRequest::Unknown });
}

/// Such as a known request with a value this build doesn't know, which comes back with its `id` to be refused.
#[test]
fn a_request_that_doesnt_decode_still_names_its_call() {
    let line = br#"{"id":7,"request":{"method":"session_approve","id":"s","approval_id":"a","decision":{"decision":"allow_always"}}}"#;

    let call = frame::read_call::<ClientRequest>(line).unwrap();

    assert_eq!(call.id, 7);
    assert!(call.request.is_err(), "it reads as a whole now");
    let known = frame::read_call::<ClientRequest>(br#"{"id":8,"request":{"method":"hub_status"}}"#).unwrap();
    assert_eq!((known.id, known.request.unwrap()), (8, ClientRequest::HubStatus));
    assert!(frame::read_call::<ClientRequest>(br#"{"request":{"method":"hub_status"}}"#).is_err(), "it names no call");
}

/// So the call it was for can be ended. Only a reply's `id` counts, since other frames may carry one of their own.
#[test]
fn the_call_a_reply_that_doesnt_decode_was_for_can_still_be_read() {
    let reply = br#"{"frame":"reply","id":3,"response":{"result":"from_a_newer_worker"}}"#;
    let event = br#"{"frame":"from_a_newer_worker","id":3}"#;

    assert!(frame::decode::<WorkerFrame>(reply).is_err(), "it reads as a whole now");
    assert_eq!(frame::reply_id(reply), Some(3));
    assert_eq!(frame::reply_id(event), None);
}
