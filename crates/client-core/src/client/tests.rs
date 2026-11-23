// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::path::Path;
use std::time::Duration;

use protocol::client::{
    ApiError, CLIENT_PROTOCOL_VERSION, ClientRequest, ClientResponse, ErrorCode, HelloReply, HubFrame, HubNotice,
    ReplyOutcome, subscriptions,
};
use protocol::terminal::Typed;

use super::*;
use crate::stand_in_hub::{self, Does, StandIn};

/// The hub answers the first request after its hello, the second this client sends, with a reply no build of this
/// client can read, and stays connected, so only the reply can end the call.
#[test]
fn a_reply_this_client_cannot_read_fails_the_call_rather_than_hanging() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let unreadable = r#"{"frame":"reply","id":2,"response":{"result":"from_a_newer_hub"}}"#;
    StandIn::of_build("older").listen(&socket, |_| Does::Writes(unreadable.to_string()));
    let client = Client::connect(&socket, "test").unwrap();

    let (finished, outcome) = flume::bounded(1);
    std::thread::spawn(move || finished.send(client.hub_status()));
    let outcome = outcome.recv_timeout(Duration::from_secs(10)).expect("the call hung");

    assert!(matches!(outcome, Err(ClientError::Unexpected(_))), "{outcome:?}");
}

/// As only a hub with a bug would answer. The connection goes on, and so does a call whose reply is that kind.
#[test]
fn a_reply_of_another_kind_than_its_request_names_fails_the_call() {
    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), |_| {
        Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Ok)))
    });

    let got = client.session_get(&"s".into());

    assert!(matches!(got, Err(ClientError::Unexpected(_))), "{got:?}");
    client.session_cancel(&"s".into()).unwrap();
}

/// Starts following the hub, then sends more notices than a subscription holds before anyone reads them.
fn start_hub_that_floods_a_subscription(socket: &Path) -> flume::Receiver<ClientRequest> {
    stand_in_hub::listen(socket, |request| {
        let ClientRequest::Subscribe(subscribe) = request else { return Does::Nothing };
        stand_in_hub::subscribed(&subscribe, |subscription_id, _| {
            let notice = HubFrame::Notice { subscription_id, notice: HubNotice::Unknown };
            vec![notice; SUBSCRIPTION_CAPACITY + 10]
        })
    })
}

#[test]
fn a_subscription_whose_reader_falls_behind_ends_rather_than_queueing_without_limit() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked = start_hub_that_floods_a_subscription(&socket);
    let client = Client::connect(&socket, "test").unwrap();
    let subscription = client.follow(subscriptions::Follow::Hub).unwrap();

    let mut asked = std::iter::from_fn(|| asked.recv_timeout(Duration::from_secs(10)).ok());
    let Some(ClientRequest::Subscribe(started)) = asked.find(|request| matches!(request, ClientRequest::Subscribe(_)))
    else {
        panic!("it never subscribed")
    };
    let next = asked.next().expect("nothing was asked after subscribing");

    // Its ID, which it may have fallen behind on before the reply came, as here, where everything comes before it.
    let stopped = subscriptions::Subscribe { stop: vec![started.start[0].subscription_id], start: Vec::new() };
    assert_eq!(next, ClientRequest::Subscribe(stopped));
    assert_eq!(subscription.items.drain().count(), SUBSCRIPTION_CAPACITY);
    assert!(subscription.items.recv().is_err(), "it ended");
    assert_eq!(subscription.ended(), Some(Ended::FellBehind));
}

/// One that chose a version this client doesn't speak, which no hub should, since a hub chooses one both speak.
#[test]
fn a_hub_that_chose_another_version_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let newer = HelloReply { protocol_version: CLIENT_PROTOCOL_VERSION + 1, hub_version: "a".into() };
    StandIn::greeting(ClientResponse::Hello(newer)).listen(&socket, |_| Does::Nothing);

    let connected = Client::connect(&socket, "test");

    assert!(
        matches!(&connected, Err(ClientError::Api(e)) if e.code == ErrorCode::VersionMismatch),
        "{:?}",
        connected.err()
    );
}

/// A hub from before clients followed only through `Subscribe`, which speaks only version 3 of the protocol, shares
/// no version with this client, so it refuses the hello, as the hub does, and the client says the hub is across a
/// protocol break from it, which only a person can do something about, by updating or replacing the hub. A desktop app
/// replaces one that it started itself, as `docs/principles/protocol.md` says.
#[test]
fn a_hub_that_speaks_only_version_3_is_too_old() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let (hub, ours) =
        (protocol::versions::Versions::only(3), protocol::versions::Versions::only(CLIENT_PROTOCOL_VERSION));
    assert_eq!(hub.newest_shared(ours), None, "the two share no version");
    let refused =
        ApiError::new(ErrorCode::VersionMismatch, format!("this hub is on {hub}, and this client speaks {ours}"));
    StandIn::refusing_hello(refused).listen(&socket, |_| Does::Nothing);

    let connected = Client::connect(&socket, "test");

    let refused = connected.err().expect("refused");
    assert!(refused.is_version_mismatch() && refused.needs_a_person(), "{refused:?}");
    assert!(refused.to_string().contains("protocol 3"), "{refused}");
}

/// A hub that takes the request to stop and goes before its reply gets out did what was asked, so stopping it is done,
/// as it is for one that had already gone.
#[test]
fn a_hub_that_goes_before_its_reply_to_stop_gets_out_has_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked = stand_in_hub::listen(&socket, |_| Does::HangsUp);

    stop_hub(&socket).unwrap();

    let first = asked.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(first, ClientRequest::HubShutdown), "it was asked to stop: {first:?}");
}

/// What a client writes with once its hub has gone, as a socket's writer is.
struct Gone;

impl std::io::Write for Gone {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A write fails once the hub has gone, which the reader may not have found yet, as when the hub's machine went away
/// without a word, so the write ends the connection, as the reader would.
#[test]
fn a_write_that_fails_ends_the_connection() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    // Kept open, so the reader waits on it, and only the write can end the connection.
    let (_hub, _) = listener.accept().unwrap();
    let reader = Box::new(stream.try_clone().unwrap());
    let transport = transport::Transport { reader, writer: Box::new(Gone), closer: transport::Closer::tcp(stream) };
    let client = Client::from_transport(transport).unwrap();

    let typed = client.terminal_typed(&"t".into(), Typed::Text { text: "x".into() });

    assert!(matches!(typed, Err(ClientError::Disconnected)), "{typed:?}");
    assert!(client.is_disconnected());
}

/// A connection knows the device its hello named, which a chat's conversation is on when its record names it, and a
/// client on no device has none.
#[test]
fn a_connection_knows_the_device_its_hello_named() {
    let dir = tempfile::tempdir().unwrap();
    let (on, off) = (dir.path().join("on.sock"), dir.path().join("off.sock"));
    stand_in_hub::listen(&on, |_| Does::Nothing);
    stand_in_hub::listen(&off, |_| Does::Nothing);
    let device = protocol::model::ClientDevice { id: "mac".into(), name: "Mac Studio".into() };

    let client = Client::connect(&on, Introduction::on("desktop", device)).unwrap();
    assert_eq!(client.device().map(protocol::model::DeviceId::as_str), Some("mac"));
    assert_eq!(Client::connect(&off, "cli").unwrap().device(), None);
}
