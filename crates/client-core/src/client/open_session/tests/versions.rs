//! Reading a session's log a version at a time, as a frame drawing each of its rows does, and hearing of each change
//! as it comes.

use crate::Hub;
use parking_lot::Mutex;
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome, log};
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::{Place, QueuedPrompt, Seq};
use protocol::prompt::Prompt;
use std::sync::Arc;

use super::super::View;
use super::super::source::SessionSource;
use super::{answered_simply, session, start_hub_answering};
use crate::Connection;
use crate::client::follow::Source as _;
use crate::session_log::SessionLog;
use protocol::prompt::PromptFrom;

/// So whatever reads a version, such as a frame drawing each of its rows from it, reads one log however much comes
/// while it does.
#[test]
fn a_version_stays_as_it_was_while_the_log_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let _heard = start_hub_answering(&socket, answered_simply);
    let client = Connection::connect(&socket, "test").unwrap();
    let hub = Hub::caught_up_on(&client);
    let open = hub.open_session(&session(), View::Own);
    let before = open.version();

    open.add_pending(QueuedPrompt {
        client_msg_id: "m1".into(),
        prompt: Prompt::new("next"),
        from: PromptFrom::Person,
    });

    assert!(before.pending().is_empty(), "the version read before stays as it was");
    assert_eq!(open.version().pending().len(), 1, "and the next has it");
    assert_eq!(open.read(|log| log.pending().len()), 1, "as reading the log now does");
}

/// So a view draws the page nearest where it's looking as soon as it's in, since it draws only a version it was told
/// of, rather than once the pages around it are in too.
#[test]
fn each_page_fetched_is_told_of_as_it_is_put_in_place() {
    // Far enough along that the pages around the one looked at are fetched rather than followed.
    const LAST: Place = Place::new(1800);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let _heard = start_hub_answering(&socket, |request| {
        if matches!(request, ClientRequest::SessionPromptList(_)) {
            return Some(ReplyOutcome::Response(ClientResponse::Prompts(log::Prompts { prompts: Vec::new() })));
        }
        let ClientRequest::SessionReadPlaces(read) = request else { return answered_simply(request) };
        let from = read.from_place.get();
        let notice = |place| {
            let body = EventBody::Notice { level: NoticeLevel::Info, message: "read".into(), subagent: None };
            protocol::fixtures::event(Seq::new(place), body)
        };
        let events = (from..from + u64::from(read.limit)).map(notice).collect();
        Some(ReplyOutcome::Response(ClientResponse::PlacedEvents(log::PlacedEvents { events, last_place: LAST })))
    });
    let client = Connection::connect(&socket, "test").unwrap();
    let hub = Hub::caught_up_on(&client);
    let feed = hub.records().unwrap();
    let mut source = SessionSource {
        session_id: "s".into(),
        view: View::Own,
        placed: true,
        read_from: Some(client.hub_version().to_string()),
        records: feed.reach(),
    };
    let mut log = SessionLog::new(LAST);
    log.focus_on(300..301, Some(300));
    let held = Mutex::new(Arc::new(log));
    let told = Mutex::new(Vec::new());

    source.asked(Some(&client), &held, &|| told.lock().push(held.lock().loaded_events())).unwrap();

    assert_eq!(*told.lock(), [250, 500, 750], "told of each page, the one looked at first, as it's put in place");
}
