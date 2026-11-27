// Against stand-in hubs on Unix sockets.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
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

/// A stand-in hub on `socket` that lists its records once `list` is sent something, and sends nothing of any other
/// level, such as a terminal, which a test opens to hear when what's open has been carried on to it. With everything it
/// hears.
fn hub_listing_after(socket: &Path, list: flume::Receiver<()>) -> (Client, flume::Receiver<ClientRequest>) {
    let heard = stand_in_hub::listen(socket, move |request| {
        let ClientRequest::Subscribe(subscribe) = &request else {
            return Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "refused"))));
        };
        let of_the_hub = subscribe.start.iter().any(|start| start.follow == Follow::Hub);
        let listed = stand_in_hub::subscribed(subscribe, |subscription_id, _| {
            Vec::from_iter(
                of_the_hub.then(|| HubFrame::Listed { subscription_id, listed: Listed::Hub(Box::default()) }),
            )
        });
        if of_the_hub { Does::After(list.clone(), Box::new(listed)) } else { listed }
    });
    (Client::connect(socket, "test").unwrap(), heard)
}

/// Whether `request` starts following `follow`.
fn follows(request: &ClientRequest, follow: impl Fn(&Follow) -> bool) -> bool {
    matches!(request, ClientRequest::Subscribe(subscribe) if subscribe.start.iter().any(|start| follow(&start.follow)))
}

/// A new connection made while the one before is still catching up, as Try again or the phone coming to the front can,
/// isn't caught up on while its own list is held back, even once the one before has caught up as it went. So nothing
/// is sent on it before then.
#[test]
fn a_connection_made_while_the_one_before_catches_up_is_caught_up_on_only_once_its_own_list_is_in() {
    let dir = tempfile::tempdir().unwrap();
    // The first lists its records at once, and answers what it offers only once the test lets it, so the records are
    // on their way to catching up on it, with its list in, as the next connection is made.
    let (offer, offered) = flume::bounded(1);
    let first_socket = dir.path().join("first.sock");
    let first_heard = stand_in_hub::listen(&first_socket, move |request| {
        if let ClientRequest::Subscribe(subscribe) = &request {
            stand_in_hub::subscribed(subscribe, |subscription_id, _| {
                vec![HubFrame::Listed { subscription_id, listed: Listed::Hub(Box::default()) }]
            })
        } else if request == ClientRequest::AgentList {
            let refused = ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "refused"));
            Does::After(offered.clone(), Box::new(Does::Answers(Box::new(refused))))
        } else {
            Does::Nothing
        }
    });
    let first = Client::connect(&first_socket, "test").unwrap();
    let (list, listed) = flume::bounded(1);
    let (next, next_heard) = hub_listing_after(&dir.path().join("next.sock"), listed);
    let link = Link::start_waiting(Tries::of(vec![Ok(first), Ok(next)]), Waits::retrying(NEVER, NEVER));
    let hub = link.hub();
    let _terminal = hub.open_terminal(&"t".into());
    waiting::thread::until(TIMEOUT, "the first never asked what it offers", || {
        first_heard.try_iter().any(|request| request == ClientRequest::AgentList)
    });

    // Without letting the first go, as a hub that's being stopped is kept for the stopping. The terminal, carried on
    // after the records, says the next connection has been handed to them.
    link.connect_after(|_| Ok(()));
    waiting::thread::until(TIMEOUT, "what's open was never carried on to the next", || {
        next_heard.try_iter().any(|request| follows(&request, |follow| matches!(follow, Follow::Terminal { .. })))
    });
    offer.send(()).unwrap();
    waiting::thread::until(TIMEOUT, "the records were never followed on the next", || {
        next_heard.try_iter().any(|request| follows(&request, |follow| *follow == Follow::Hub))
    });
    let (records, caught_up) = (hub.records().unwrap().connected(), hub.caught_up());
    list.send(()).unwrap();

    assert!(!records, "the records said they'd caught up, as on the first, while the next's list was held");
    assert!(!caught_up, "caught up on the next while its list was held");
    waiting::thread::until(TIMEOUT, "never caught up once its list was in", || hub.caught_up());
}
