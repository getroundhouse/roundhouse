//! Pairing a phone, and when the hub listens on the network for one.

#![cfg(test)]

use std::io::Write as _;
use std::os::unix::net::UnixStream;

use client_core::{Client, ClientError, Fingerprint, PairingLink, RemoteHub};
use protocol::PROTOCOL_VERSION;
use protocol::client::{ClientFrame, ClientRequest, ErrorCode};
use protocol::frame;

use crate::support::{CLOSED, Stack, pair_phone, pairing_link, wait_until};

#[test]
fn a_code_pairs_one_phone_once() {
    let stack = Stack::start();
    let client = stack.client();
    let (link, code) = pairing_link(&client);

    let phone = client_core::pair(&link, "Ada's iPhone").unwrap();

    let paired = client.pairing_wait(&code).unwrap();
    assert_eq!((paired.id.as_str(), paired.name.as_str()), (phone.device_id.as_str(), "Ada's iPhone"));
    let again = client_core::pair(&link, "another phone").unwrap_err();
    assert!(again.is_unauthorized(), "{again:?}");
    let devices = client.device_list().unwrap();
    assert_eq!(devices.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), [phone.device_id.as_str()]);
}

/// Otherwise a `roundhouse phone pair` stopped with Ctrl-C would count as a client until its code expired, and the
/// app wouldn't replace an idle hub meanwhile.
#[test]
fn waiting_on_a_code_stops_when_whoever_waits_goes() {
    let stack = Stack::start();
    let client = stack.client();
    let pairing = client.pairing_start().unwrap();
    let mut waiting = UnixStream::connect(stack.socket()).unwrap();
    let hello = ClientRequest::Hello { protocol_version: PROTOCOL_VERSION, client: "cli".to_string() };
    for (id, request) in [(1, hello), (2, ClientRequest::PairingWait { code: pairing.code })] {
        waiting.write_all(&frame::encode(&ClientFrame { id, request }).unwrap()).unwrap();
    }
    wait_until("the wait to start", || client.hub_status().unwrap().requests == 2);

    drop(waiting);

    wait_until("the hub to count only this client", || client.hub_status().unwrap().clients == 1);
}

#[test]
fn a_hub_with_nothing_paired_listens_only_while_a_code_is_live() {
    // Long enough to see the hub listening on a busy machine, and short enough to wait out.
    let stack = Stack::start_with(vec![(hub::ENV_PAIRING_LIFE_MS, "2000".into())]);
    let client = stack.client();
    assert_eq!(client.hub_status().unwrap().network_port, None);

    let (link, code) = pairing_link(&client);

    let expired = client.pairing_wait(&code).unwrap_err();
    assert!(matches!(&expired, ClientError::Api(e) if e.code == ErrorCode::NotFound), "{expired:?}");
    wait_until("the hub to stop listening", || client.hub_status().unwrap().network_port.is_none());
    // Another test's hub can have the port by now, but not this hub's key.
    let gone = Client::open_remote(&link.hosts, &link.fingerprint).map(drop);
    assert!(gone.is_err(), "this hub still answers at {}", link.hosts[0]);
}

#[test]
fn a_hub_with_a_phone_paired_listens_from_when_it_starts() {
    let mut stack = Stack::start();
    pair_phone(&stack.client(), "phone");
    stack.kill_hub();

    stack.start_hub();

    assert!(stack.client().hub_status().unwrap().network_port.is_some(), "listening for the phone that's paired");
}

#[test]
fn a_phone_refuses_a_hub_with_another_key() {
    let stack = Stack::start();
    let client = stack.client();
    let (link, _) = pairing_link(&client);
    let another_key = Fingerprint::of_sha256([0; 32]);

    let refused = client_core::pair(&PairingLink { fingerprint: another_key.clone(), ..link.clone() }, "phone");

    assert!(matches!(&refused, Err(ClientError::FingerprintMismatch { .. })), "{refused:?}");
    // It refused before sending anything, so the code still works.
    let phone = client_core::pair(&link, "phone").unwrap();
    let moved = RemoteHub { fingerprint: another_key, ..phone };
    let refused = Client::connect_remote(&moved, "phone").map(drop);
    assert!(matches!(&refused, Err(ClientError::FingerprintMismatch { .. })), "{refused:?}");
}

/// Such as when the hub's Mac is asleep, and another Mac running a hub now has an address it had. Pairing again
/// wouldn't help, and might pair with the wrong hub.
#[test]
fn a_phone_that_reaches_only_a_hub_with_another_key_says_it_can_not_reach_its_own() {
    let stack = Stack::start();
    let client = stack.client();
    let paired = pair_phone(&client, "phone");
    let other = paired.hosts[0].clone();
    let hosts = vec![CLOSED.to_string(), other.clone()];

    let unreachable =
        Client::connect_remote(&RemoteHub { hosts, fingerprint: Fingerprint::of_sha256([0; 32]), ..paired }, "phone");

    let unreachable = unreachable.map(drop).unwrap_err();
    assert!(matches!(&unreachable, ClientError::Unreachable { not_pinned, .. } if *not_pinned == [other]));
    assert!(unreachable.to_string().contains("with another key"), "{unreachable}");
}
