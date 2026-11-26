// Against stand-in hubs on Unix sockets.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::{
    ApiError, CLIENT_PROTOCOL_VERSION, ClientRequest, ErrorCode, HubFrame, HubRefusing, ReplyOutcome,
};
use protocol::versions::Versions;

use super::*;
use crate::stand_in_hub::{self, Does};
use crate::{Client, ClientError};

const TIMEOUT: Duration = Duration::from_secs(10);
/// Longer than any test runs, so a test that passes didn't wait.
const NEVER: Duration = Duration::from_secs(600);

/// What each try gets, in turn: a connection, or a failure. Once they've all been tried, each try fails as a hub
/// that can't be reached does.
#[derive(Clone, Default)]
struct Tries {
    next: Arc<Mutex<Vec<Result<Client, ClientError>>>>,
    /// When each try was made, and what was done first, as `after` said.
    made: Arc<Mutex<Vec<(Instant, &'static str)>>>,
}

impl Tries {
    fn of(each: Vec<Result<Client, ClientError>>) -> Tries {
        let tries = Tries::default();
        tries.next.lock().extend(each.into_iter().rev());
        tries
    }

    fn count(&self) -> usize {
        self.made.lock().len()
    }
}

impl Connect for Tries {
    type With = usize;
    type Failure = String;

    fn connect(&mut self) -> Result<(Client, usize), Refused<String>> {
        self.made.lock().push((Instant::now(), "try"));
        let which = self.count();
        match self.next.lock().pop().unwrap_or(Err(ClientError::Disconnected)) {
            Ok(client) => Ok((client, which)),
            Err(error) => Err(Refused::by(&error, error.to_string())),
        }
    }

    fn lost(&self, lost: &Lost) -> String {
        lost.to_string()
    }
}

/// A stand-in hub on `socket` that sends its records, and goes when it's asked for its repositories, which a test
/// asks for to lose the connection.
fn hub(socket: &Path) -> Client {
    stand_in_hub::connect(socket, |request| {
        let answer = if let ClientRequest::Subscribe(subscribe) = &request {
            return stand_in_hub::subscribed(subscribe, |subscription_id, _| {
                vec![HubFrame::Listed { subscription_id, listed: Listed::Hub(Box::default()) }]
            });
        } else if request == ClientRequest::AgentList {
            ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "refused"))
        } else if request == ClientRequest::RepoList {
            return Does::HangsUp;
        } else {
            return Does::Nothing;
        };
        Does::Answers(Box::new(answer))
    })
}

/// A stand-in hub on `socket` that takes a client following it, and goes as it's next asked something, before it has
/// said where its records stand, so before the link has caught up on them.
fn hub_that_goes_as_it_lists(socket: &Path) -> Client {
    stand_in_hub::connect(socket, |request| {
        let ClientRequest::Subscribe(subscribe) = &request else { return Does::HangsUp };
        stand_in_hub::subscribed(subscribe, |_, _| Vec::new())
    })
}

/// A stand-in hub on `socket` that never answers, so a link never catches up on it.
fn silent_hub(socket: &Path) -> Client {
    stand_in_hub::connect(socket, |_| Does::Nothing)
}

fn refused(code: ErrorCode) -> ClientError {
    ClientError::Api(ApiError::new(code, "refused"))
}

/// The refusal of a hub that speaks `speaks`, or that doesn't say what it speaks.
fn across_a_break(speaks: Option<Versions>) -> ClientError {
    let said = |speaks| ApiError::versions_differ("refused", HubRefusing { version: "0.1.9".into(), speaks });
    speaks.map_or_else(|| refused(ErrorCode::VersionMismatch), |speaks| said(speaks).into())
}

fn ready_on<C: Connect>(link: &Link<C>, which: &str) {
    waiting::thread::until(TIMEOUT, format!("never ready on {which}"), || link.state().ready().is_some());
}

#[test]
fn the_wait_before_trying_again_doubles_from_a_second_up_to_thirty() {
    let waits: Vec<u64> = (0..8).map(|failures| Waits::EVERY_CLIENT.before_try(failures).as_secs()).collect();

    assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30, 30]);
}

/// A revoked token, another key or being older than the hub across a protocol break stay until a person does something,
/// and anything else may not, such as an older hub, which someone may update.
#[test]
fn only_what_trying_again_cant_fix_needs_a_person() {
    let newer = Versions { oldest: CLIENT_PROTOCOL_VERSION + 1, newest: CLIENT_PROTOCOL_VERSION + 2 };
    let older = Versions { oldest: CLIENT_PROTOCOL_VERSION - 2, newest: CLIENT_PROTOCOL_VERSION - 1 };
    let cases = [
        ("a revoked token", refused(ErrorCode::Unauthorized), true),
        ("a protocol break from a newer hub", across_a_break(Some(newer)), true),
        ("a protocol break from an older hub", across_a_break(Some(older)), false),
        ("a protocol break from a hub that doesn't say what it speaks", across_a_break(None), false),
        ("another key", ClientError::FingerprintMismatch { host: "a".into() }, true),
        ("unreachable", ClientError::Unreachable { hosts: Vec::new(), last: None, not_pinned: Vec::new() }, false),
        ("gone", ClientError::Disconnected, false),
        ("refused otherwise", refused(ErrorCode::Internal), false),
        ("a reply it didn't expect", ClientError::Unexpected("odd".into()), false),
    ];

    for (name, error, needs_a_person) in cases {
        assert_eq!(error.needs_a_person(), needs_a_person, "{name}");
    }
}

#[test]
fn a_failure_trying_again_may_fix_is_tried_again_after_each_wait() {
    let tries = Tries::of(Vec::new());
    let waits = Waits::retrying(Duration::from_millis(20), Duration::from_millis(80));

    let link = Link::start_waiting(tries.clone(), waits);

    waiting::thread::until(TIMEOUT, "it stopped trying", || tries.count() == 5);
    let made: Vec<Instant> = tries.made.lock().iter().map(|(at, _)| *at).collect();
    let gaps: Vec<Duration> = made.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let least = [20, 40, 80, 80].map(Duration::from_millis);
    assert!(gaps.iter().zip(least).all(|(gap, least)| *gap >= least), "{gaps:?}");
    assert!(matches!(link.state(), LinkState::Waiting(_) | LinkState::Connecting(Some(_))));
}

#[test]
fn a_failure_only_a_person_can_fix_waits_until_the_link_is_asked_to_connect() {
    let dir = tempfile::tempdir().unwrap();
    let tries = Tries::of(vec![Err(refused(ErrorCode::Unauthorized)), Ok(hub(&dir.path().join("hub.sock")))]);
    let link = Link::start_waiting(tries.clone(), Waits::retrying(Duration::ZERO, Duration::ZERO));
    waiting::thread::until(TIMEOUT, "it never stopped", || matches!(link.state(), LinkState::Stopped(_)));
    waiting::thread::for_what_mustnt_happen(Duration::from_millis(100));
    assert_eq!(tries.count(), 1, "tried again by itself");

    link.connect_now();

    ready_on(&link, "the second try");
}

#[test]
fn a_connection_that_goes_is_made_again_at_once_and_the_records_carry_on_on_the_next() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (hub(&dir.path().join("first.sock")), hub(&dir.path().join("second.sock")));
    let tries = Tries::of(vec![Ok(first.clone()), Ok(second)]);
    let link = Link::start_waiting(tries.clone(), Waits::retrying(NEVER, NEVER));
    ready_on(&link, "the first");
    let (_, which) = link.take_connection().unwrap();
    let feed = link.hub().records().unwrap();

    let _ = first.repo_list();

    waiting::thread::until(TIMEOUT, "never connected again", || tries.count() == 2);
    ready_on(&link, "the second");
    assert_eq!((which, link.take_connection().map(|(_, which)| which)), (1, Some(2)));
    assert!(feed.connected(), "the same records, on the new connection");
}

#[test]
fn a_new_connection_that_closes_before_catching_up_is_made_again_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let goes = hub_that_goes_as_it_lists(&dir.path().join("first.sock"));
    let tries = Tries::of(vec![Ok(goes), Ok(hub(&dir.path().join("second.sock")))]);

    let link = Link::start_waiting(tries.clone(), Waits::retrying(NEVER, NEVER));

    ready_on(&link, "the second");
    assert_eq!(tries.count(), 2);
}

#[test]
fn a_new_connection_that_doesnt_catch_up_in_time_is_dropped_and_tried_again() {
    let dir = tempfile::tempdir().unwrap();
    let silent = silent_hub(&dir.path().join("first.sock"));
    let tries = Tries::of(vec![Ok(silent), Ok(hub(&dir.path().join("second.sock")))]);
    let waits = Waits::retrying(Duration::ZERO, Duration::ZERO).catching_up_within(Duration::from_millis(100));

    let link = Link::start_waiting(tries.clone(), waits);

    ready_on(&link, "the second");
    assert_eq!(tries.count(), 2);
}

/// Such as stopping a hub so another can start, which no try may race.
#[test]
fn what_comes_before_connecting_again_is_done_before_any_try() {
    let dir = tempfile::tempdir().unwrap();
    let tries = Tries::of(vec![Err(refused(ErrorCode::Unauthorized)), Ok(hub(&dir.path().join("hub.sock")))]);
    let link = Link::start_waiting(tries.clone(), Waits::retrying(Duration::ZERO, Duration::ZERO));
    waiting::thread::until(TIMEOUT, "it never stopped", || matches!(link.state(), LinkState::Stopped(_)));
    let made = tries.made.clone();

    link.connect_after(move |_| {
        made.lock().push((Instant::now(), "first"));
        Ok(())
    });

    ready_on(&link, "the try after");
    let order: Vec<&str> = tries.made.lock().iter().map(|(_, what)| *what).collect();
    assert_eq!(order, ["try", "first", "try"]);
}

#[test]
fn what_comes_before_connecting_again_failing_waits_for_a_person() {
    let tries = Tries::of(vec![Err(refused(ErrorCode::Unauthorized))]);
    let link = Link::start_waiting(tries.clone(), Waits::retrying(Duration::ZERO, Duration::ZERO));
    waiting::thread::until(TIMEOUT, "it never stopped", || matches!(link.state(), LinkState::Stopped(_)));

    link.connect_after(|_| Err("couldn't stop it".to_string()));

    waiting::thread::until(
        TIMEOUT,
        "never said why",
        || matches!(link.state(), LinkState::Stopped(why) if why == "couldn't stop it"),
    );
    assert_eq!(tries.count(), 1);
}
