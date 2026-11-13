//! Following a session whose record says a request waits that no page the view loads holds, as one a subagent in the
//! background asked long before.

use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome, log};
use protocol::model::{OpenApproval, Seq, Session};

use super::super::OpenSession;
use super::{LAST_SEQ, answered_simply, session, start_hub_answering, until};
use crate::Client;
use crate::session_log::RecordedApprovals;

/// Pages before the newest two, which a view opened at the end doesn't load.
const ASKED_AT: Seq = 9;

#[test]
fn a_chat_opened_while_a_request_from_long_before_waits_fetches_that_request() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let heard = start_hub_answering(&socket, |request| {
        if matches!(request, ClientRequest::SessionRead(log::SessionRead { from_seq: ASKED_AT, limit: 1, .. })) {
            let asked = protocol::fixtures::event(ASKED_AT, protocol::fixtures::approval_request("z", "Bash").into());
            Some(ReplyOutcome::Response(ClientResponse::Events(log::Events {
                events: vec![asked],
                last_seq: LAST_SEQ,
            })))
        } else if matches!(request, ClientRequest::SessionPromptList(_)) {
            Some(ReplyOutcome::Response(ClientResponse::Prompts(log::Prompts { prompts: Vec::new() })))
        } else {
            answered_simply(request)
        }
    });
    let waiting = vec![OpenApproval { seq: ASKED_AT, approval_id: "z".into() }];
    let session = Session { open_approvals: Some(waiting), ..session() };
    let recorded = RecordedApprovals::of(&session);

    let open = OpenSession::start(&Client::connect(&socket, "test").unwrap(), &session);

    let asked = || open.read(|log| log.open_approvals(recorded.as_ref()).iter().map(|e| e.seq).collect::<Vec<_>>());
    until("the request is fetched", || asked() == [ASKED_AT]);
    let read = heard
        .try_iter()
        .filter(|r| matches!(r, ClientRequest::SessionRead(log::SessionRead { from_seq: ASKED_AT, .. })));
    assert_eq!(read.count(), 1, "once, by itself");
}
