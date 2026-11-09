//! What a paired phone can do over the network, and what it can't.

#![cfg(test)]

use std::net::TcpListener;
use std::time::{Duration, Instant};

use client_core::records::HubRecords;
use client_core::session_log::{Row, SessionLog};
use client_core::{Client, ClientError, Ended, OpenSession, RecordsFeed, RemoteHub, WriteOnly};
use protocol::PROTOCOL_VERSION;
use protocol::client::{ClientRequest, ErrorCode};
use protocol::event::SessionEvent;

use crate::support::{Stack, TIMEOUT, assistant_text, pair_phone, wait_for_turn_end, wait_until};

fn connections(client: &Client) -> u32 {
    client.device_list().unwrap().iter().map(|device| device.connections).sum()
}

fn held(log: &SessionLog) -> Vec<SessionEvent> {
    (0..log.row_count())
        .filter_map(|row| match log.row(row) {
            Row::Event(event) => Some(event.clone()),
            Row::Unloaded | Row::Pending(_) => None,
        })
        .collect()
}

#[test]
fn a_paired_phone_follows_the_hub_as_a_client_on_its_machine_does() {
    let stack = Stack::start();
    let client = stack.client();
    let phone = Client::connect_remote(&pair_phone(&client, "phone"), "phone").unwrap();
    let records = RecordsFeed::start(&phone);

    let (workspace, session) = stack.workspace(&client);
    let open = OpenSession::start(&phone, &session);
    phone.session_prompt(&session.id, "say:hello from the phone", None).unwrap();

    let has_workspace = |records: &HubRecords| records.workspaces().iter().any(|w| w.id == workspace.id);
    wait_until("the phone to see the workspace", || records.read(has_workspace));
    wait_until("the phone to see the reply", || assistant_text(&open.read(held)).contains("hello from the phone"));
    assert_eq!(phone.hub_version(), protocol::BUILD_VERSION);
    assert_eq!(phone.records_list().unwrap().workspaces, client.records_list().unwrap().workspaces);
    assert_eq!(connections(&client), 1);
    drop((open, records, phone));
    wait_until("the phone's connection to close", || connections(&client) == 0);
}

/// Such as a `.local` name that takes seconds to look up, or gives an address this phone can't reach. Trying one
/// address at a time, the phone would wait its connect or handshake out first.
#[test]
fn a_phone_connects_at_once_when_an_address_before_the_hub_never_answers() {
    let stack = Stack::start();
    let client = stack.client();
    let paired = pair_phone(&client, "phone");
    // Accepts, since the system does that, but never says anything.
    let silent = TcpListener::bind("127.0.0.1:0").unwrap();
    let hosts = vec![silent.local_addr().unwrap().to_string(), paired.hosts[0].clone()];
    let started = Instant::now();
    drop(Client::connect_remote(&paired, "phone").unwrap());
    let alone = started.elapsed();

    let started = Instant::now();
    let phone = Client::connect_remote(&RemoteHub { hosts, ..paired }, "phone");

    let took = started.elapsed();
    assert!(phone.is_ok(), "{:?}", phone.map(drop));
    // Waiting the silent host out would take 5 s more. A quarter of a second more is the wait before trying the next.
    assert!(took < alone + Duration::from_millis(2500), "took {took:?}, and {alone:?} with only the hub's host");
}

#[test]
fn a_wrong_token_is_refused() {
    let stack = Stack::start();
    let client = stack.client();
    let paired = pair_phone(&client, "phone");

    let refused = Client::connect_remote(&RemoteHub { token: "not-its-token".to_string(), ..paired.clone() }, "phone");

    assert!(refused.as_ref().is_err_and(ClientError::is_unauthorized), "{:?}", refused.map(drop));
    assert!(Client::connect_remote(&paired, "phone").is_ok());
}

#[test]
fn a_revoked_phone_is_cut_off_at_once_and_refused_after() {
    let stack = Stack::start();
    let client = stack.client();
    let (kept, revoked) = (pair_phone(&client, "kept"), pair_phone(&client, "revoked"));
    let phone = Client::connect_remote(&revoked, "phone").unwrap();
    let feed = phone.feed_subscribe().unwrap();

    client.device_revoke(&revoked.device_id.as_str().into()).unwrap();

    while let Ok(_notice) = feed.items.recv_timeout(TIMEOUT) {}
    assert_eq!(feed.ended(), Some(Ended::Disconnected));
    let refused = Client::connect_remote(&revoked, "phone").map(drop);
    assert!(refused.as_ref().is_err_and(ClientError::is_unauthorized), "{refused:?}");
    assert!(Client::connect_remote(&kept, "phone").is_ok(), "only the revoked phone is refused");
    // With no phone left, the hub stops listening.
    client.device_revoke(&kept.device_id.as_str().into()).unwrap();
    assert_eq!(client.hub_status().unwrap().network_port, None);
    let gone = Client::connect_remote(&kept, "phone").map(drop);
    // Another test's hub can have the port by now, with its own key.
    let refused = matches!(gone, Err(ClientError::Unreachable { .. } | ClientError::FingerprintMismatch { .. }));
    assert!(refused, "{gone:?}");
}

/// However the connection is waiting. Here the phone asked for far more than the system holds between the two, never
/// read it, and then sent something that ends the connection, so all that's left is to send what it won't take.
#[test]
fn a_revoked_phone_is_cut_off_even_while_the_hub_waits_for_it_to_read() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // A megabyte the fake agent ignores, since it isn't a command.
    client.session_prompt(&session.id, &format!("say:ok;{}", "x".repeat(1 << 20)), None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let paired = pair_phone(&client, "phone");
    let mut phone = WriteOnly::open(&paired.hosts, &paired.fingerprint).unwrap();
    phone.send(ClientRequest::Authenticate { token: paired.token.as_str().into() }).unwrap();
    phone.send(ClientRequest::Hello { protocol_version: PROTOCOL_VERSION, client: "phone".to_string() }).unwrap();
    // About a megabyte each.
    let read =
        ClientRequest::SessionRead { id: session.id, from_seq: 1, limit: 100, max_bytes: None, include_raw: true };
    for _ in 0..48 {
        phone.send(read.clone()).unwrap();
    }
    phone.send_bytes(b"not a frame\n").unwrap();
    wait_until("the phone to count as a client", || client.hub_status().unwrap().clients == 2);
    // Counting the request that asks.
    wait_until("the hub to answer everything", || client.hub_status().unwrap().requests == 1);

    client.device_revoke(&paired.device_id.as_str().into()).unwrap();

    wait_until("the revoked phone to stop counting", || client.hub_status().unwrap().clients == 1);
}

#[test]
fn a_phone_can_neither_see_nor_stop_the_hub_nor_choose_its_devices() {
    let stack = Stack::start();
    let client = stack.client();
    let paired = pair_phone(&client, "phone");
    let phone = Client::connect_remote(&paired, "phone").unwrap();

    let refused = [
        phone.hub_status().map(drop),
        phone.hub_shutdown(),
        phone.pairing_start().map(drop),
        phone.device_list().map(drop),
        phone.device_revoke(&paired.device_id.as_str().into()),
    ];

    for refused in refused {
        assert!(matches!(&refused, Err(ClientError::Api(e)) if e.code == ErrorCode::LocalOnly), "{refused:?}");
    }
    assert!(client.hub_status().is_ok(), "the hub is still running");
    assert_eq!(client.device_list().unwrap().len(), 1);
    assert!(phone.repo_list().is_ok(), "the phone's connection carries on");
}

#[test]
fn a_connection_from_the_network_that_does_not_authenticate_first_is_closed() {
    let stack = Stack::start();
    let client = stack.client();
    let paired = pair_phone(&client, "phone");
    let hello = ClientRequest::Hello { protocol_version: PROTOCOL_VERSION, client: "phone".to_string() };

    for first in [ClientRequest::HubStatus, ClientRequest::HubShutdown, hello, ClientRequest::RepoList] {
        let connection = Client::open_remote(&paired.hosts, &paired.fingerprint).unwrap();

        let refused = connection.call(first.clone()).map(drop);

        assert!(refused.as_ref().is_err_and(ClientError::is_unauthorized), "{first:?}: {refused:?}");
        let closed = connection.call(ClientRequest::RepoList).map(drop);
        assert!(matches!(closed, Err(ClientError::Disconnected)), "{first:?}: {closed:?}");
    }
    assert!(client.hub_status().is_ok(), "the hub is still running");
}
