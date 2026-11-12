//! The hellos, which are how two versions find one they both speak, so they keep their form even across a real
//! break. A hello from before ranges names only its version, and still has to be answered.

use protocol::client::{ClientRequest, ClientResponse};
use protocol::frame;
use protocol::worker::{WorkerRequest, WorkerResponse};

#[test]
fn a_clients_hello_keeps_its_form_with_its_range_and_without() {
    let hello = ClientRequest::Hello { protocol_version: 4, client: "c".into(), oldest_protocol_version: Some(3) };
    let line = br#"{"method":"hello","protocol_version":4,"client":"c","oldest_protocol_version":3}"#;
    assert_eq!(frame::encode(&hello).unwrap(), [&line[..], b"\n"].concat());

    let before_ranges: ClientRequest =
        frame::decode(br#"{"method":"hello","protocol_version":3,"client":"c"}"#).unwrap();
    assert_eq!(
        before_ranges,
        ClientRequest::Hello { protocol_version: 3, client: "c".into(), oldest_protocol_version: None }
    );

    let reply: ClientResponse = frame::decode(br#"{"result":"hello","protocol_version":3,"hub_version":"a"}"#).unwrap();
    assert_eq!(reply, ClientResponse::Hello { protocol_version: 3, hub_version: "a".into() });
}

#[test]
fn a_hubs_hello_to_a_worker_keeps_its_form_with_its_range_and_without() {
    let hello = WorkerRequest::Hello { protocol_version: 4, oldest_protocol_version: Some(3) };
    let line = br#"{"method":"hello","protocol_version":4,"oldest_protocol_version":3}"#;
    assert_eq!(frame::encode(&hello).unwrap(), [&line[..], b"\n"].concat());

    let before_ranges: WorkerRequest = frame::decode(br#"{"method":"hello","protocol_version":3}"#).unwrap();
    assert_eq!(before_ranges, WorkerRequest::Hello { protocol_version: 3, oldest_protocol_version: None });

    let reply: WorkerResponse = frame::decode(br#"{"result":"hello","protocol_version":3}"#).unwrap();
    assert_eq!(reply, WorkerResponse::Hello { protocol_version: 3 });
}
