use std::net::Ipv4Addr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use super::*;

fn tools() -> Arc<Tools> {
    let (frames, _) = mpsc::channel(8);
    Arc::new(Tools::new(Arc::new(Asks::new(frames))))
}

/// A port nothing is on, as of just now, which the system chooses.
async fn free_port() -> u16 {
    let port = listen(0, false).unwrap().local_addr().unwrap().port();
    until_nothing_answers(port).await;
    port
}

/// Waits until nothing answers on `port`. A process another test starts just as a socket is made can inherit it, since
/// on a Mac a socket is marked not to be inherited only a moment after it's made, and then the port answers until that
/// process exits.
async fn until_nothing_answers(port: u16) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while answered(port).await {
        assert!(std::time::Instant::now() < deadline, "something went on answering on {port}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn it_serves_on_the_port_it_was_on_before_and_says_the_same_once_serving() {
    let tools = tools();
    let port = free_port().await;

    let serving = tools.serve(Some(port)).await.unwrap();
    let again = tools.serve(Some(1)).await.unwrap();

    assert_eq!((serving, again), (port, port));
}

/// The hub can ask it to serve and then go, so the call can come after the worker has started stopping, and a server
/// started then would keep it running.
#[tokio::test]
async fn once_stopped_it_wont_serve_again() {
    let tools = tools();

    tools.stop().await;
    let refused = tools.serve(None).await;

    assert!(refused.is_err());
    assert!(matches!(*tools.serving.lock().await, Serving::Stopped));
}

#[tokio::test]
async fn a_port_another_program_listens_on_is_left_to_it() {
    let tools = tools();
    // On every address, which a listener that may reuse its port could still listen over on a Mac.
    let theirs = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await.unwrap();
    let port = theirs.local_addr().unwrap().port();

    let serving = tools.serve(Some(port)).await.unwrap();

    assert_ne!(serving, port);
    let mut reached = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap();
    reached.write_all(b"theirs").await.unwrap();
    drop(reached);
    // Looking for a listener there connected and went, so that comes first.
    let mut heard = Vec::new();
    while heard.is_empty() {
        let (mut accepted, _) = theirs.accept().await.unwrap();
        accepted.read_to_end(&mut heard).await.unwrap();
    }
    assert_eq!(heard, b"theirs", "what's sent to their port still reaches them");
}

/// The worker's own listener, on a port below the range the system gives whoever asks for any port, as other tests
/// and their connections do, so none of them can be given it while it's closing. That's from 49152 on a Mac, and from
/// 32768 on Linux.
fn listener_of_its_own() -> TcpListener {
    let start = u16::try_from(uuid::Uuid::new_v4().as_u128() % 10_000).unwrap();
    (0..10_000)
        .map(|i| 20_000 + (start + i) % 10_000)
        .find_map(|port| listen(port, false).ok())
        .expect("a port from 20000 to 29999")
}

/// Whether a socket that doesn't say it may reuse its port can have `port`, which it can't while connections on it are
/// closing. On Linux, the worker's own listener always says so.
fn binds_without_reuse(port: u16) -> bool {
    TcpSocket::new_v4().unwrap().bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok()
}

#[tokio::test]
async fn a_port_whose_connections_are_still_closing_is_had_again() {
    let listener = listener_of_its_own();
    let port = listener.local_addr().unwrap().port();
    let client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap();
    let (server, _) = listener.accept().await.unwrap();
    // The server's end closes first, as when a worker exits under its agents, so the port waits out its close.
    drop(server);
    drop(listener);
    drop(client);
    until_nothing_answers(port).await;
    assert!(!binds_without_reuse(port), "the port isn't closing, so this tests nothing");

    let serving = tools().serve(Some(port)).await.unwrap();

    assert_eq!(serving, port);
}

#[tokio::test]
async fn a_port_is_chosen_when_there_was_none_before_and_freed_once_it_stops() {
    let tools = tools();

    let port = tools.serve(None).await.unwrap();
    tools.stop().await;

    assert_ne!(port, 0);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.is_ok() {
        assert!(std::time::Instant::now() < deadline, "it went on serving");
        tokio::task::yield_now().await;
    }
}
