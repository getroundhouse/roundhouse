//! Pairing a device and reaching a hub over the network, as the requests and replies carry them, spelled out. A phone
//! keeps its token and the hub it paired with updates apart from it, so they meet builds of any age.

use protocol::client::devices::{Authenticate, DeviceRevoke, Devices, Pair, Paired, PairingWait};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::frame;
use protocol::pairing::{Device, DeviceToken, Pairing, PairingCode, PairingLink};

use super::{answered, asked};

const LINK: &str =
    "roundhouse://pair?v=1&name=hub&fp=q3B0ZWxsZXJzLWZpbmdlcnByaW50LXNoYTI1Ni1oYXN&code=c0de&host=hub.local:7867";

const DEVICE: &str = r#"{"id":"d","name":"Phone","paired_at_ms":1,"last_seen_ms":5,"connections":2}"#;

fn device() -> Device {
    Device { id: "d".into(), name: "Phone".into(), paired_at_ms: 1, last_seen_ms: 5, connections: 2 }
}

#[test]
fn every_request_about_pairing_a_device_keeps_its_form() {
    let pair = Pair { code: PairingCode::from("c0de"), device_name: "Phone".into() };
    asked(ClientRequest::Pair(pair), r#"{"method":"pair","code":"c0de","device_name":"Phone"}"#);
    let authenticate = Authenticate { token: DeviceToken::from("a-made-up-token") };
    asked(ClientRequest::Authenticate(authenticate), r#"{"method":"authenticate","token":"a-made-up-token"}"#);
    asked(ClientRequest::PairingStart, r#"{"method":"pairing_start"}"#);
    asked(
        ClientRequest::PairingWait(PairingWait { code: PairingCode::from("c0de") }),
        r#"{"method":"pairing_wait","code":"c0de"}"#,
    );
    asked(ClientRequest::DeviceList, r#"{"method":"device_list"}"#);
    asked(ClientRequest::DeviceRevoke(DeviceRevoke { id: "d".into() }), r#"{"method":"device_revoke","id":"d"}"#);
}

#[test]
fn every_reply_about_pairing_a_device_keeps_its_form() {
    let paired = Paired { device_id: "d".into(), token: DeviceToken::from("a-made-up-token") };
    answered(ClientResponse::Paired(paired), r#"{"result":"paired","device_id":"d","token":"a-made-up-token"}"#);
    let pairing =
        Pairing { link: PairingLink::parse(LINK).unwrap(), code: PairingCode::from("c0de"), expires_at_ms: 9 };
    answered(
        ClientResponse::Pairing(pairing),
        &format!(r#"{{"result":"pairing","link":"{LINK}","code":"c0de","expires_at_ms":9}}"#),
    );
    answered(ClientResponse::Device(device()), &format!(r#"{{"result":"device",{}"#, &DEVICE[1..]));
    answered(
        ClientResponse::Devices(Devices { devices: vec![device()] }),
        &format!(r#"{{"result":"devices","devices":[{DEVICE}]}}"#),
    );
}

#[test]
fn a_device_that_doesnt_count_its_connections_has_none() {
    let uncounted = r#"{"result":"device","id":"d","name":"Phone","paired_at_ms":1,"last_seen_ms":5}"#;

    let reply: ClientResponse = frame::decode(uncounted.as_bytes()).unwrap();

    assert_eq!(reply, ClientResponse::Device(Device { connections: 0, ..device() }));
}
