// Against stand-in hubs on Unix sockets.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::records::{RepoList, Repos};
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};

use crate::link::{Connect, Link, LinkState, Lost, Refused, Waits};
use crate::stand_in_hub::{self, Does};
use crate::{Client, ClientError, DraftChange, NotTaken, Opening, View, Written};
use protocol::prompt::Prompt;

const TIMEOUT: Duration = Duration::from_secs(10);
/// Longer than any test runs, so the link tries again only when it's asked to.
const NEVER: Duration = Duration::from_secs(600);

/// Each try gets the next of these, in turn, and fails as a hub that can't be reached does once they've all been tried.
#[derive(Clone)]
struct Tries(Arc<Mutex<Vec<Result<Client, ClientError>>>>);

impl Tries {
    fn of(each: Vec<Result<Client, ClientError>>) -> Tries {
        Tries(Arc::new(Mutex::new(each.into_iter().rev().collect())))
    }
}

impl Connect for Tries {
    type With = ();
    type Failure = String;

    fn connect(&mut self) -> Result<(Client, ()), Refused<String>> {
        match self.0.lock().pop().unwrap_or(Err(ClientError::Disconnected)) {
            Ok(client) => Ok((client, ())),
            Err(error) => Err(Refused::by(&error, error.to_string())),
        }
    }

    fn lost(&self, lost: &Lost) -> String {
        lost.to_string()
    }
}

/// What a hub does with `subscribe`: lists its records where it starts following the hub, and sends nothing of the
/// rest, such as a terminal.
fn listing(subscribe: &Subscribe) -> Does {
    stand_in_hub::subscribed(subscribe, |subscription_id, follow| {
        let listed = (*follow == Follow::Hub).then(|| Listed::Hub(Box::default()));
        Vec::from_iter(listed.map(|listed| HubFrame::Listed { subscription_id, listed }))
    })
}

/// What a hub does with what a test isn't about, such as the records asking which agents it offers.
fn doesnt_know() -> Does {
    Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "refused"))))
}

/// A stand-in hub on `socket` that lists its records once `list` is sent something, and answers that it has no
/// repositories, with everything it hears.
fn hub_that_lists_after(socket: &Path, list: flume::Receiver<()>) -> (Client, flume::Receiver<ClientRequest>) {
    let heard = stand_in_hub::listen(socket, move |request| {
        if let ClientRequest::Subscribe(subscribe) = &request {
            let follows_the_hub = subscribe.start.iter().any(|start| start.follow == Follow::Hub);
            return if follows_the_hub {
                Does::After(list.clone(), Box::new(listing(subscribe)))
            } else {
                listing(subscribe)
            };
        }
        if request != ClientRequest::RepoList {
            return doesnt_know();
        }
        Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Repos(Repos { repos: Vec::new() }))))
    });
    (Client::connect(socket, "test").unwrap(), heard)
}

/// A stand-in hub on `socket` that lists its records at once, and goes when it's asked for its repositories, which a
/// test asks for to lose the connection, with everything it hears.
fn hub_that_goes_when_asked(socket: &Path) -> (Client, flume::Receiver<ClientRequest>) {
    let heard = stand_in_hub::listen(socket, |request| {
        if let ClientRequest::Subscribe(subscribe) = &request {
            listing(subscribe)
        } else if request == ClientRequest::RepoList {
            Does::HangsUp
        } else {
            doesnt_know()
        }
    });
    (Client::connect(socket, "test").unwrap(), heard)
}

/// Waits for `heard` to have had a request that starts following a terminal, and one that starts following a chat, in
/// any order.
fn followed(heard: &flume::Receiver<ClientRequest>, on: &str) {
    let (mut terminal, mut session) = (false, false);
    waiting::thread::until(TIMEOUT, format!("the terminal and the chat were never followed on {on}"), || {
        for request in heard.try_iter() {
            // A chat asks how far its events go before it follows them, which the stand-in doesn't know.
            session |= matches!(request, ClientRequest::SessionReadPlaces(_));
            let ClientRequest::Subscribe(subscribe) = request else { continue };
            terminal |= subscribe.start.iter().any(|start| matches!(start.follow, Follow::Terminal { .. }));
        }
        terminal && session
    });
}

/// What's asked while a new connection catches up on the records isn't sent, then or later, so it can't act on records
/// the first list is about to replace. It fails at once, as with no connection, and once it's caught up it's sent. A
/// prompt sent through an open chat is the same, handed back at once, and so are what's asked once what's typed has
/// been kept, and opening a file. The round moves only once it has caught up.
#[test]
fn a_request_is_sent_only_on_a_connection_that_has_caught_up() {
    let dir = tempfile::tempdir().unwrap();
    let (list, listed) = flume::bounded(1);
    let (client, heard) = hub_that_lists_after(&dir.path().join("hub.sock"), listed);
    let link = Link::start_waiting(Tries::of(vec![Ok(client)]), Waits::retrying(NEVER, NEVER));
    let hub = link.hub();
    let catching_up = || matches!(link.state(), LinkState::CatchingUp(_));
    waiting::thread::until(TIMEOUT, "never connected", catching_up);
    let chat = hub.open_session(&protocol::fixtures::session("s"), View::Own);
    let round_catching_up = hub.round();

    let while_catching_up = hub.ask(RepoList).wait();
    let prompt_while_catching_up = chat.start_sending(Written::new(Prompt::new("hello"))).wait();
    let once_kept_while_catching_up = hub.ask_once_kept(RepoList).wait();
    let file = hub.open_file(&"w".into(), &"c".into(), "f");
    waiting::thread::until(TIMEOUT, "the file never failed", || {
        file.read(|opening| matches!(opening, Opening::Failed))
    });
    list.send(()).unwrap();
    waiting::thread::until(TIMEOUT, "never caught up", || hub.caught_up());
    let once_caught_up = hub.ask(RepoList).wait();

    assert!(matches!(while_catching_up, Err(ClientError::Disconnected)), "{while_catching_up:?}");
    assert!(matches!(once_kept_while_catching_up, Err(ClientError::Disconnected)), "{once_kept_while_catching_up:?}");
    assert!(matches!(file.take_errors().as_slice(), [ClientError::Disconnected]), "the file wasn't asked for");
    let Err(NotTaken { error: ClientError::Disconnected, prompt }) = prompt_while_catching_up else {
        panic!("the prompt wasn't handed back: {prompt_while_catching_up:?}")
    };
    assert_eq!(prompt.text, "hello");
    assert_eq!(once_caught_up.unwrap(), Repos { repos: Vec::new() });
    let heard: Vec<ClientRequest> = heard.try_iter().collect();
    let asked = heard.iter().filter(|request| **request == ClientRequest::RepoList).count();
    assert_eq!(asked, 1, "only what was asked once it had caught up was sent");
    assert!(!heard.iter().any(|request| matches!(request, ClientRequest::SessionPrompt(_))), "the prompt was sent");
    assert_ne!(hub.round(), round_catching_up, "a connection that has caught up is a new round");
}

/// What's asked once what's typed has been kept, as a comment saved from what's typed of it, isn't sent until the hub
/// has answered for what's typed, and is sent then.
#[test]
fn a_request_asked_once_what_is_typed_is_kept_waits_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let (keep, kept) = flume::bounded(1);
    let heard = stand_in_hub::listen(&socket, move |request| {
        if let ClientRequest::Subscribe(subscribe) = &request {
            listing(subscribe)
        } else if request == ClientRequest::RepoList {
            Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Repos(Repos { repos: Vec::new() }))))
        } else if matches!(request, ClientRequest::WorkspaceCommentDraft(_)) {
            Does::After(kept.clone(), Box::new(doesnt_know()))
        } else {
            doesnt_know()
        }
    });
    let client = Client::connect(&socket, "test").unwrap();
    let link = Link::start_waiting(Tries::of(vec![Ok(client)]), Waits::retrying(NEVER, NEVER));
    let hub = link.hub();
    waiting::thread::until(TIMEOUT, "never caught up", || hub.caught_up());
    hub.drafts().set(DraftChange::Comment("w".into(), None));
    let comment_draft = |request: &ClientRequest| matches!(request, ClientRequest::WorkspaceCommentDraft(_));
    waiting::thread::until(TIMEOUT, "what's typed never went", || heard.try_iter().any(|r| comment_draft(&r)));

    let asked = hub.ask_once_kept(RepoList);
    waiting::thread::for_what_mustnt_happen(Duration::from_millis(100));
    let before = heard.try_iter().filter(|request| *request == ClientRequest::RepoList).count();
    keep.send(()).unwrap();

    assert_eq!(before, 0, "asked before what's typed was kept");
    assert_eq!(asked.wait().unwrap(), Repos { repos: Vec::new() });
}

/// A terminal and a chat opened before there's a connection follow on the first, and on each after it, as after the
/// hub restarts, with nothing but the hub to carry them on.
#[test]
fn what_is_opened_from_the_hub_follows_on_each_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (first, first_heard) = hub_that_goes_when_asked(&dir.path().join("first.sock"));
    let (second, second_heard) = hub_that_goes_when_asked(&dir.path().join("second.sock"));
    let refused = ClientError::Api(ApiError::new(ErrorCode::Unauthorized, "refused"));
    let tries = Tries::of(vec![Err(refused), Ok(first), Ok(second)]);
    let link = Link::start_waiting(tries, Waits::retrying(NEVER, NEVER));
    waiting::thread::until(TIMEOUT, "it never stopped", || matches!(link.state(), LinkState::Stopped(_)));
    let hub = link.hub();

    let open = hub.open_terminal(&"t".into());
    let _chat = hub.open_session(&protocol::fixtures::session("s"), View::Own);
    link.connect_now();
    followed(&first_heard, "the first connection");
    waiting::thread::until(TIMEOUT, "never caught up", || hub.caught_up());
    drop(hub.ask(RepoList));

    followed(&second_heard, "the second connection");
    waiting::thread::until(TIMEOUT, "never followed there", || open.connected());
}

/// Where the hub is doesn't change with its connection, so a hub reached by its socket is on this machine once that
/// connection has gone, as it is while it's there. Before any connection, nothing says so.
#[test]
fn a_hub_is_on_this_machine_as_its_newest_connection_said_once_that_has_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (first, _) = hub_that_goes_when_asked(&dir.path().join("hub.sock"));
    let refused = ClientError::Api(ApiError::new(ErrorCode::Unauthorized, "refused"));
    let link = Link::start_waiting(Tries::of(vec![Err(refused), Ok(first)]), Waits::retrying(NEVER, NEVER));
    let hub = link.hub();
    waiting::thread::until(TIMEOUT, "it never stopped", || matches!(link.state(), LinkState::Stopped(_)));
    let before_any = hub.on_this_machine();
    link.connect_now();
    waiting::thread::until(TIMEOUT, "never caught up", || hub.caught_up());

    drop(hub.ask(RepoList));

    waiting::thread::until(TIMEOUT, "the connection never went", || !hub.caught_up());
    assert!(!before_any, "nothing says where it is before a connection");
    assert!(hub.on_this_machine(), "on this machine, as the connection that went said");
}
