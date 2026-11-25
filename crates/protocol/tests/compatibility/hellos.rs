//! The hellos, which are how two versions find one they both speak, so they keep their form even across a real
//! break. A hello from before ranges names only its version, and still has to be answered.

use protocol::client::{self, ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubRefusing, ReplyOutcome};
use protocol::frame;
use protocol::model::ClientDevice;
use protocol::versions::Versions;
use protocol::worker::{Hello, HelloReply, WorkerRequest, WorkerResponse};

#[test]
fn a_clients_hello_keeps_its_form_with_its_range_and_without() {
    let ranges = ClientRequest::Hello(client::Hello {
        protocol_version: 4,
        client: "c".into(),
        oldest_protocol_version: Some(3),
        device: None,
    });
    let line = br#"{"method":"hello","protocol_version":4,"client":"c","oldest_protocol_version":3}"#;
    assert_eq!(frame::encode(&ranges).unwrap(), [&line[..], b"\n"].concat());

    let before_ranges: ClientRequest =
        frame::decode(br#"{"method":"hello","protocol_version":3,"client":"c"}"#).unwrap();
    assert_eq!(
        before_ranges,
        ClientRequest::Hello(client::Hello {
            protocol_version: 3,
            client: "c".into(),
            oldest_protocol_version: None,
            device: None
        })
    );

    let reply: ClientResponse = frame::decode(br#"{"result":"hello","protocol_version":3,"hub_version":"a"}"#).unwrap();
    assert_eq!(reply, ClientResponse::Hello(client::HelloReply { protocol_version: 3, hub_version: "a".into() }));
}

/// A hello can only gain fields that older peers ignore, so one saying which device the client runs on still finds a
/// version with every hub, which reads only the versions, and the forms above still read as they did.
#[test]
fn a_clients_hello_that_says_its_device_keeps_the_rest_of_its_form() {
    let hello = ClientRequest::Hello(client::Hello {
        protocol_version: 4,
        client: "desktop".into(),
        oldest_protocol_version: Some(4),
        device: Some(ClientDevice { id: "d1".into(), name: "Ada's MacBook Air".into() }),
    });
    let line = br#"{"method":"hello","protocol_version":4,"client":"desktop","oldest_protocol_version":4,"device":{"id":"d1","name":"Ada's MacBook Air"}}"#;
    assert_eq!(frame::encode(&hello).unwrap(), [&line[..], b"\n"].concat());
    assert_eq!(frame::decode::<ClientRequest>(line).unwrap(), hello);
}

/// A client reads it across a break, to say which side is older. A hub from before it said which release it's of, and
/// what it speaks, refuses with neither.
#[test]
fn a_hubs_refusal_of_a_hello_keeps_its_form_saying_which_hub_it_is_and_not() {
    let hub = HubRefusing { version: "0.1.9".into(), speaks: Versions { oldest: 5, newest: 6 } };
    let says = HubFrame::Reply { id: 1, outcome: ReplyOutcome::Error(ApiError::versions_differ("no", hub)) };
    let line = br#"{"frame":"reply","id":1,"error":{"code":"version_mismatch","message":"no","hub":{"version":"0.1.9","speaks":{"oldest":5,"newest":6}}}}"#;
    assert_eq!(frame::encode(&says).unwrap(), [&line[..], b"\n"].concat());
    assert_eq!(frame::decode::<HubFrame>(line).unwrap(), says);

    let from_before: HubFrame =
        frame::decode(br#"{"frame":"reply","id":1,"error":{"code":"version_mismatch","message":"no"}}"#).unwrap();
    let refused = ApiError::new(ErrorCode::VersionMismatch, "no");
    assert_eq!(from_before, HubFrame::Reply { id: 1, outcome: ReplyOutcome::Error(refused) });
}

#[test]
fn a_hubs_hello_to_a_worker_keeps_its_form_with_its_range_and_without() {
    let hello = WorkerRequest::Hello(Hello {
        protocol_version: 4,
        oldest_protocol_version: Some(3),
        keep_running_without_hub: true,
    });
    let line =
        br#"{"method":"hello","protocol_version":4,"oldest_protocol_version":3,"keep_running_without_hub":true}"#;
    assert_eq!(frame::encode(&hello).unwrap(), [&line[..], b"\n"].concat());

    let before_ranges: WorkerRequest = frame::decode(br#"{"method":"hello","protocol_version":3}"#).unwrap();
    let before = WorkerRequest::Hello(Hello {
        protocol_version: 3,
        oldest_protocol_version: None,
        keep_running_without_hub: false,
    });
    assert_eq!(before_ranges, before, "and before a hub could ask its worker to keep running without it");

    let reply: WorkerResponse = frame::decode(br#"{"result":"hello","protocol_version":3}"#).unwrap();
    assert_eq!(reply, WorkerResponse::Hello(HelloReply { protocol_version: 3, build: None }));
    let named: WorkerResponse = frame::decode(br#"{"result":"hello","protocol_version":3,"build":"0.1.9"}"#).unwrap();
    assert_eq!(named, WorkerResponse::Hello(HelloReply { protocol_version: 3, build: Some("0.1.9".into()) }));
}
