// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::time::Duration;

use super::*;
use crate::stand_in_hub::{self, Does};

/// Answers a hello, then answers the next request with a reply no build of this client can read.
fn start_hub_with_an_unreadable_reply(socket: &Path) {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut requests = BufReader::new(stream.try_clone().unwrap()).lines();
        requests.next();
        let outcome = ReplyOutcome::Response(stand_in_hub::hello_reply("older"));
        stream.write_all(&frame::encode(&HubFrame::Reply { id: 1, outcome }).unwrap()).unwrap();
        requests.next();
        stream.write_all(b"{\"frame\":\"reply\",\"id\":2,\"response\":{\"result\":\"from_a_newer_hub\"}}\n").unwrap();
        // Stays connected, so that only the reply can end the call.
        requests.next();
    });
}

#[test]
fn a_reply_this_client_cannot_read_fails_the_call_rather_than_hanging() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    start_hub_with_an_unreadable_reply(&socket);
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

/// Answers a hello and a feed subscription, then sends more notices than a subscription holds before
/// anyone reads them, and reports what the client asks next.
fn start_hub_that_floods_a_subscription(socket: &Path) -> flume::Receiver<ClientRequest> {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    let (asked, next) = flume::unbounded();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut requests = BufReader::new(stream.try_clone().unwrap()).lines();
        let mut reply = |id, response| {
            let outcome = ReplyOutcome::Response(response);
            stream.write_all(&frame::encode(&HubFrame::Reply { id, outcome }).unwrap()).unwrap();
        };
        let mut read = || frame::decode::<ClientFrame>(requests.next().unwrap().unwrap().as_bytes()).unwrap();
        let hello = read();
        reply(hello.id, stand_in_hub::hello_reply("a"));
        let subscribe = read();
        let ClientRequest::FeedSubscribe(subscriptions::FeedSubscribe { subscription_id }) = subscribe.request else {
            panic!()
        };
        reply(subscribe.id, ClientResponse::Ok);
        for _ in 0..SUBSCRIPTION_CAPACITY + 10 {
            let notice = HubFrame::Notice { subscription_id, notice: HubNotice::Resync };
            stream.write_all(&frame::encode(&notice).unwrap()).unwrap();
        }
        asked.send(read().request).unwrap();
    });
    next
}

#[test]
fn a_subscription_whose_reader_falls_behind_ends_rather_than_queueing_without_limit() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked = start_hub_that_floods_a_subscription(&socket);
    let client = Client::connect(&socket, "test").unwrap();
    let subscription = client.feed_subscribe().unwrap();

    let next = asked.recv_timeout(Duration::from_secs(10)).unwrap();

    assert_eq!(
        next,
        ClientRequest::SessionUnsubscribe(subscriptions::SessionUnsubscribe { subscription_id: subscription.id })
    );
    assert_eq!(subscription.items.drain().count(), SUBSCRIPTION_CAPACITY);
    assert!(subscription.items.recv().is_err(), "it ended");
    assert_eq!(subscription.ended(), Some(Ended::FellBehind));
}

/// One that chose a version this client doesn't speak, which no hub should, since a hub chooses one both speak.
#[test]
fn a_hub_that_chose_another_version_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut requests = BufReader::new(stream.try_clone().unwrap()).lines();
        requests.next();
        let newer = ClientResponse::Hello(HelloReply {
            protocol_version: CLIENT_PROTOCOL_VERSION + 1,
            hub_version: "a".into(),
        });
        let outcome = ReplyOutcome::Response(newer);
        stream.write_all(&frame::encode(&HubFrame::Reply { id: 1, outcome }).unwrap()).unwrap();
        requests.next();
    });

    let connected = Client::connect(&socket, "test");

    assert!(
        matches!(&connected, Err(ClientError::Api(e)) if e.code == ErrorCode::VersionMismatch),
        "{:?}",
        connected.err()
    );
}

/// A hub that takes the request to stop and goes before its reply gets out did what was asked, so stopping it is done,
/// as it is for one that had already gone.
#[test]
fn a_hub_that_goes_before_its_reply_to_stop_gets_out_has_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let hub = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let asked = std::io::BufRead::lines(std::io::BufReader::new(&stream)).next().unwrap().unwrap();
        serde_json::from_str::<protocol::client::ClientFrame>(&asked).unwrap().request
    });

    stop_hub(&socket).unwrap();

    assert!(matches!(hub.join().unwrap(), ClientRequest::HubShutdown), "it was asked to stop");
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

    let typed = client.terminal_input(&"t".into(), b"x");

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
