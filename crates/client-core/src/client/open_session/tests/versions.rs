//! Reading a session's log a version at a time, as a frame drawing each of its rows does, and hearing of each change
//! as it comes.

use parking_lot::Mutex;
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome, log};
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::{QueuedPrompt, Seq};
use protocol::prompt::Prompt;
use std::sync::Arc;

use super::super::loading::SessionSource;
use super::super::{OpenSession, View};
use super::{LAST_SEQ, answered_simply, session, start_hub_answering};
use crate::Client;
use crate::client::follow::Source as _;
use crate::session_log::SessionLog;

/// So whatever reads a version, such as a frame drawing each of its rows from it, reads one log however much comes
/// while it does.
#[test]
fn a_version_stays_as_it_was_while_the_log_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let _heard = start_hub_answering(&socket, answered_simply);
    let open = OpenSession::start(&Client::connect(&socket, "test").unwrap(), &session());
    let before = open.version();

    open.add_pending(QueuedPrompt { client_msg_id: "m1".into(), prompt: Prompt::new("next") });

    assert!(before.pending().is_empty(), "the version read before stays as it was");
    assert_eq!(open.version().pending().len(), 1, "and the next has it");
    assert_eq!(open.read(|log| log.pending().len()), 1, "as reading the log now does");
}

/// So a view draws the page nearest where it's looking as soon as it's in, since it draws only a version it was told
/// of, rather than once the pages around it are in too.
#[test]
fn each_page_fetched_is_told_of_as_it_is_put_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let _heard = start_hub_answering(&socket, |request| {
        if matches!(request, ClientRequest::SessionPromptList(_)) {
            return Some(ReplyOutcome::Response(ClientResponse::Prompts(log::Prompts { prompts: Vec::new() })));
        }
        let ClientRequest::SessionRead(read) = request else { return answered_simply(request) };
        let from = read.from_seq.get();
        let notice = |seq| {
            let body = EventBody::Notice { level: NoticeLevel::Info, message: "read".into(), subagent: None };
            protocol::fixtures::event(Seq::new(seq), body)
        };
        let events = (from..from + u64::from(read.limit)).map(notice).collect();
        Some(ReplyOutcome::Response(ClientResponse::Events(log::Events { events, last_seq: LAST_SEQ })))
    });
    let client = Client::connect(&socket, "test").unwrap();
    let mut source = SessionSource {
        session_id: "s".into(),
        wanted: View::Whole,
        view: View::Whole,
        last_seq: LAST_SEQ,
        placed: true,
        read_from: client.hub_version().to_string(),
    };
    let mut log = SessionLog::new(LAST_SEQ);
    log.focus_on(300..301, Some(300));
    let held = Mutex::new(Arc::new(log));
    let told = Mutex::new(Vec::new());

    source.asked(&client, &held, &|| told.lock().push(held.lock().loaded_events())).unwrap();

    assert_eq!(*told.lock(), [250, 500, 750], "told of each page, the one looked at first, as it's put in place");
}
