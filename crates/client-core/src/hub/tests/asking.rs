//! What's asked of the `Hub` and opened from it, on each connection, against stand-in hubs.

use std::path::Path;
use std::time::Duration;

use protocol::client::records::{RepoList, Repos};
use protocol::client::subscriptions::Follow;
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome};
use protocol::model::{ChangedFileAt, FileDiff, FileDiffBody, FileVersion};
use protocol::prompt::Prompt;

use super::stand_ins::{NEVER, TIMEOUT, Tries, doesnt_know, listing};
use crate::link::{Link, LinkState, Waits};
use crate::stand_in_hub::{self, Does};
use crate::{ClientError, Connection, DraftChange, Hub, NotTaken, Opening, View, Written};
use protocol::client::{ApiError, ErrorCode};

/// A stand-in hub on `socket` that lists its records once `list` is sent something, and answers that it has no
/// repositories, with everything it hears.
fn hub_that_lists_after(socket: &Path, list: flume::Receiver<()>) -> (Connection, flume::Receiver<ClientRequest>) {
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
    (Connection::connect(socket, "test").unwrap(), heard)
}

/// A stand-in hub on `socket` that lists its records at once, and goes when it's asked for its repositories, which a
/// test asks for to lose the connection, with everything it hears.
fn hub_that_goes_when_asked(socket: &Path) -> (Connection, flume::Receiver<ClientRequest>) {
    let heard = stand_in_hub::listen(socket, |request| {
        if let ClientRequest::Subscribe(subscribe) = &request {
            listing(subscribe)
        } else if request == ClientRequest::RepoList {
            Does::HangsUp
        } else {
            doesnt_know()
        }
    });
    (Connection::connect(socket, "test").unwrap(), heard)
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
    let catching_up = || matches!(link.state(), LinkState::CatchingUp);
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
    let client = Connection::connect(&socket, "test").unwrap();
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

/// A stand-in hub on `socket` that answers a file's diff in three pages, the second once `second` is sent something,
/// with everything it hears.
fn hub_paging_after(socket: &Path, second: flume::Receiver<()>) -> (Connection, flume::Receiver<ClientRequest>) {
    let heard = stand_in_hub::listen(socket, move |request| {
        let page = |patch: &str, next_line| {
            let body = FileDiffBody::Patch { patch: patch.into(), next_line };
            let diff = FileDiff { base_commit: "c".into(), version: Some(FileVersion::from("v1".to_string())), body };
            Box::new(ReplyOutcome::Response(ClientResponse::FileDiff(diff)))
        };
        if let ClientRequest::Subscribe(subscribe) = &request {
            return listing(subscribe);
        }
        let ClientRequest::WorkspaceFileDiff(asked) = &request else { return doesnt_know() };
        match asked.first_line {
            0 => Does::Answers(page("one\n", Some(1))),
            1 => Does::After(second.clone(), Box::new(Does::Answers(page("two\n", Some(2))))),
            _ => Does::Answers(page("three\n", None)),
        }
    });
    (Connection::connect(socket, "test").unwrap(), heard)
}

/// Every page of a file's diff is read on the connection the first was, so a new connection while it's read doesn't
/// make one patch of pages two connections read.
#[test]
fn every_page_of_a_file_s_diff_is_read_on_one_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (release, second) = flume::bounded(1);
    let (first, first_heard) = hub_paging_after(&dir.path().join("first.sock"), second.clone());
    let (next, next_heard) = hub_paging_after(&dir.path().join("next.sock"), second);
    let hub = Hub::caught_up_on(&first);
    let at = ChangedFileAt { untracked: false, path: "f".into() };
    let reading = {
        let hub = hub.clone();
        std::thread::spawn(move || hub.read_file_diff(&"w".into(), &at, None, None))
    };
    let is_page =
        |request: &ClientRequest, line| matches!(request, ClientRequest::WorkspaceFileDiff(a) if a.first_line == line);
    waiting::thread::until(TIMEOUT, "the second page was never asked", || {
        first_heard.try_iter().any(|request| is_page(&request, 1))
    });
    hub.carry_on_to(&next);
    release.send(()).unwrap();

    let read = reading.join().unwrap().unwrap().unwrap();
    assert_eq!(read.body, FileDiffBody::Patch { patch: "one\ntwo\nthree\n".into(), next_line: None });
    let on_the_next = next_heard.try_iter().any(|request| matches!(request, ClientRequest::WorkspaceFileDiff(_)));
    assert!(!on_the_next, "a page was read on the next connection");
}
