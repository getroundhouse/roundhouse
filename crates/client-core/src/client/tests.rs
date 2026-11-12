// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::time::Duration;

use super::*;

/// Answers a hello, then answers the next request with a reply no build of this client can read.
fn start_hub_with_an_unreadable_reply(socket: &Path) {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut requests = BufReader::new(stream.try_clone().unwrap()).lines();
        requests.next();
        let hello =
            ClientResponse::Hello { protocol_version: CLIENT_PROTOCOL_VERSION, hub_version: "older".to_string() };
        let outcome = ReplyOutcome::Response(hello);
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
        reply(hello.id, ClientResponse::Hello { protocol_version: CLIENT_PROTOCOL_VERSION, hub_version: "a".into() });
        let subscribe = read();
        let ClientRequest::FeedSubscribe { subscription_id } = subscribe.request else { panic!() };
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

    assert_eq!(next, ClientRequest::SessionUnsubscribe { subscription_id: subscription.id });
    assert_eq!(subscription.items.drain().count(), SUBSCRIPTION_CAPACITY);
    assert!(subscription.items.recv().is_err(), "it ended");
    assert_eq!(subscription.ended(), Some(Ended::FellBehind));
}
