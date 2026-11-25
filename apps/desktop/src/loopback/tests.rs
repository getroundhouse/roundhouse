//! The listener alone, on real sockets on `127.0.0.1`, asked as a browser would ask it. What it listens for is made up
//! here: a word, at `/heard`.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use super::browsing::fetch;
use super::{Answer, Asked, Bound, Ended, Listening, Query, page};

/// Longer than any test here waits for what it's waiting on.
const NEVER: Duration = Duration::from_secs(600);
/// How long a test waits for the listener to do what it should at once, at most.
const SOON: Duration = Duration::from_secs(5);

/// A listener for a word, until `limit`.
fn listening(limit: Duration) -> (Listening<String>, SocketAddr) {
    let bound = Bound::new().unwrap();
    let address = bound.address();
    let answers = |asked: &Asked| match (asked.path.as_str(), asked.query.value("word")) {
        ("/heard", Some(heard)) => {
            Answer::Heard { heard, taken: page::said("It's <taken>"), late: page::said("It's late") }
        }
        _ => Answer::nothing_here(),
    };
    (bound.serve(answers, limit).unwrap(), address)
}

/// Waits until nothing listens at `address` any more.
fn until_refused(address: SocketAddr) {
    waiting::thread::until(SOON, format!("still listening at {address}"), || TcpStream::connect(address).is_err());
}

#[test]
fn it_listens_only_on_loopback_on_the_first_port_thats_free_and_otherwise_on_one_the_system_picks() {
    let taken = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let taken_too = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = |listener: &TcpListener| listener.local_addr().unwrap().port();
    let free = {
        let freed = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        port(&freed)
    };
    let ports = [port(&taken), free, port(&taken_too)];

    let first = Bound::first_free(&ports).unwrap();
    let second = Bound::first_free(&ports).unwrap();

    assert_eq!(first.address().to_string(), format!("127.0.0.1:{free}"), "not every address this machine has");
    assert_eq!(second.address().ip().to_string(), "127.0.0.1");
    assert!(!ports.contains(&second.address().port()), "two at once, with the rest taken");
}

#[test]
fn it_hears_what_its_listening_for_once_and_then_stops() {
    let (listening, address) = listening(NEVER);

    let back = fetch(&format!("http://{address}/heard?word=a%2Bb+c"));

    assert_eq!((back.status, back.body), (200, page::said("It's <taken>")));
    assert_eq!(listening.ended().recv_timeout(SOON).unwrap(), Ended::Heard("a+b c".to_string()));
    until_refused(address);
}

#[test]
fn anything_else_is_refused_and_it_goes_on_listening() {
    let (listening, address) = listening(NEVER);

    for target in ["/", "/favicon.ico", "/heard/again?word=a", "/?word=a", "/heard", "/heard?word=a&word=b"] {
        assert_eq!(fetch(&format!("http://{address}{target}")).status, 404, "{target}");
    }

    assert!(listening.ended().try_recv().is_err(), "it took none of them");
    assert_eq!(fetch(&format!("http://{address}/heard?word=a")).status, 200, "and still takes the right one");
}

#[test]
fn only_a_get_is_asked_about_with_its_query_as_it_came() {
    let asked = Asked::of("GET /heard?word=a%2Bb+c&said=1&said=2 HTTP/1.1").unwrap();

    assert_eq!(asked.path, "/heard");
    assert_eq!(asked.query.as_str(), "word=a%2Bb+c&said=1&said=2");
    assert_eq!(asked.query.value("word").as_deref(), Some("a+b c"), "as a form's encoding has it");
    assert_eq!(asked.query.value("said"), None, "since nothing says which of the two");
    assert_eq!(Asked::of("GET / HTTP/1.0").unwrap().query, Query::new(""));
    for line in ["POST /heard?word=a HTTP/1.1", "GET /heard?word=a HTTP/2", "GET /heard", "GET /a b HTTP/1.1"] {
        assert_eq!(Asked::of(line), None, "{line}");
    }
}

#[test]
fn what_a_request_asks_leaves_its_query_out_of_a_log() {
    let asked = Asked::of("GET /heard?code=a-made-up-code HTTP/1.1").unwrap();

    assert!(!format!("{asked:?}").contains("a-made-up-code"));
}

#[test]
fn it_stops_once_its_let_go_of_having_heard_nothing() {
    let (listening, address) = listening(NEVER);
    let ended = listening.ended();

    drop(listening);

    until_refused(address);
    assert_eq!(ended.recv_timeout(SOON), Err(flume::RecvTimeoutError::Disconnected), "it says nothing as it goes");
}

#[test]
fn it_stops_once_its_limit_has_passed_saying_so() {
    let (listening, address) = listening(Duration::from_millis(50));

    assert_eq!(listening.ended().recv_timeout(SOON).unwrap(), Ended::TimedOut);

    until_refused(address);
}
