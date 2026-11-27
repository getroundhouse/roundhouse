//! What outlasts a connection, over a `Hub` that has carried on to another: the calls quitting waits for, the prompts
//! on their way, and those being queued, each until it's answered or its connection goes.

use std::path::Path;
use std::time::Duration;

use protocol::client::records::{RepoList, Repos};
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome};
use protocol::prompt::Prompt;

use super::stand_ins::{TIMEOUT, doesnt_know, listing};
use crate::stand_in_hub::{self, Does};
use crate::{ClientError, Connection, Hub, View, Written};

/// A stand-in hub on `socket` that holds what a test waits on, a list of its repositories, a prompt and a prompt
/// queued, until `release` is sent something, then does with it what `then` says, with everything it hears.
fn hub_holding(
    socket: &Path,
    release: flume::Receiver<()>,
    then: fn() -> Does,
) -> (Connection, flume::Receiver<ClientRequest>) {
    let heard = stand_in_hub::listen(socket, move |request| {
        if let ClientRequest::Subscribe(subscribe) = &request {
            listing(subscribe)
        } else if repo_list(&request) || prompt(&request) || queued(&request) {
            Does::After(release.clone(), Box::new(then()))
        } else {
            doesnt_know()
        }
    });
    (Connection::connect(socket, "test").unwrap(), heard)
}

/// What answers a list of repositories, and refuses a prompt, which answers it all the same.
fn answers() -> Does {
    Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Repos(Repos { repos: Vec::new() }))))
}

/// A `Hub` caught up on a stand-in that holds what `asks` asks, as [`hub_holding`] says, and carried on to another
/// once the first has heard it, with what `asks` returned and what lets the first go on.
struct CarriedOn<T> {
    hub: Hub,
    asked: T,
    release: flume::Sender<()>,
    _dir: tempfile::TempDir,
}

fn carried_on_after<T>(
    then: fn() -> Does,
    asks: impl FnOnce(&Hub) -> T,
    heard: fn(&ClientRequest) -> bool,
) -> CarriedOn<T> {
    let dir = tempfile::tempdir().unwrap();
    let (release, released) = flume::bounded(1);
    let (first, first_heard) = hub_holding(&dir.path().join("first.sock"), released, then);
    let (_, never) = flume::bounded(1);
    let (next, _) = hub_holding(&dir.path().join("next.sock"), never, then);
    let hub = Hub::caught_up_on(&first);
    let asked = asks(&hub);
    waiting::thread::until(TIMEOUT, "the first connection never heard it", || {
        first_heard.try_iter().any(|r| heard(&r))
    });
    hub.carry_on_to(&next);
    CarriedOn { hub, asked, release, _dir: dir }
}

fn repo_list(request: &ClientRequest) -> bool {
    *request == ClientRequest::RepoList
}

fn prompt(request: &ClientRequest) -> bool {
    matches!(request, ClientRequest::SessionPrompt(_))
}

fn queued(request: &ClientRequest) -> bool {
    matches!(request, ClientRequest::SessionQueue(_))
}

/// Quitting waits for a call started on the connection before, once the `Hub` has carried on to another, until it's
/// answered.
#[test]
fn the_calls_a_hub_waits_for_include_one_started_on_the_connection_before() {
    let on = carried_on_after(answers, |hub| hub.ask(RepoList), repo_list);
    let finished = on.hub.commands().finished();
    waiting::thread::for_what_mustnt_happen(Duration::from_millis(100));
    let before = finished.try_recv().is_ok();
    on.release.send(()).unwrap();

    assert!(!before, "finished with the call on the connection before still under way");
    finished.recv_timeout(TIMEOUT).expect("never finished once it was answered");
    assert!(on.asked.wait().is_ok());
}

/// A prompt sent on the connection before is still on its way once the `Hub` has carried on to another, until it's
/// answered.
#[test]
fn a_prompt_sent_on_the_connection_before_is_on_its_way_until_it_s_answered() {
    let session = protocol::fixtures::session("s");
    let send = |hub: &Hub| {
        let chat = hub.open_session(&session, View::Own);
        (chat.start_sending(Written::new(Prompt::new("hello"))), chat)
    };
    let on = carried_on_after(answers, send, prompt);
    let before = on.hub.is_sending(&session.id);
    on.release.send(()).unwrap();

    assert!(before, "not on its way once the hub carried on");
    let (sending, _chat) = on.asked;
    let _ = sending.wait();
    assert!(!on.hub.is_sending(&session.id), "still on its way once answered");
}

/// A prompt being queued on the connection before shows as being queued once the `Hub` has carried on to another,
/// until it's answered.
#[test]
fn a_prompt_queued_on_the_connection_before_shows_until_it_s_answered() {
    let session = protocol::fixtures::session("s");
    let on =
        carried_on_after(answers, |hub| hub.start_queueing(&session.id, Written::new(Prompt::new("later"))), queued);
    let before = on.hub.queueing(&session).len();
    on.release.send(()).unwrap();

    assert_eq!(before, 1, "not shown once the hub carried on");
    let _ = on.asked.wait();
    assert!(on.hub.queueing(&session).is_empty(), "still shown once answered");
}

/// A call started on the connection before, which goes with it unanswered, is waited for no more.
#[test]
fn a_call_whose_connection_goes_unanswered_is_waited_for_no_more() {
    let on = carried_on_after(|| Does::HangsUp, |hub| hub.ask(RepoList), repo_list);
    let finished = on.hub.commands().finished();
    on.release.send(()).unwrap();

    finished.recv_timeout(TIMEOUT).expect("still waited for once its connection went");
    assert!(matches!(on.asked.wait(), Err(ClientError::Disconnected)));
}

/// A prompt sent on the connection before, which goes with it unanswered, is on its way no more.
#[test]
fn a_prompt_whose_connection_goes_unanswered_is_on_its_way_no_more() {
    let session = protocol::fixtures::session("s");
    let send = |hub: &Hub| {
        let chat = hub.open_session(&session, View::Own);
        (chat.start_sending(Written::new(Prompt::new("hello"))), chat)
    };
    let on = carried_on_after(|| Does::HangsUp, send, prompt);
    on.release.send(()).unwrap();

    let (sending, _chat) = on.asked;
    assert!(sending.wait().is_err(), "taken, with its connection gone");
    assert!(!on.hub.is_sending(&session.id), "still on its way once its connection went");
}

/// A prompt being queued on the connection before, which goes with it unanswered, shows no more.
#[test]
fn a_prompt_queued_whose_connection_goes_unanswered_shows_no_more() {
    let session = protocol::fixtures::session("s");
    let queue = |hub: &Hub| hub.start_queueing(&session.id, Written::new(Prompt::new("later")));
    let on = carried_on_after(|| Does::HangsUp, queue, queued);
    on.release.send(()).unwrap();

    assert!(on.asked.wait().is_err(), "taken, with its connection gone");
    assert!(on.hub.queueing(&session).is_empty(), "still shown once its connection went");
}
