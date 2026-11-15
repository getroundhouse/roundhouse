//! Reading a session's log a version at a time, as a frame drawing each of its rows does.

use protocol::model::QueuedPrompt;
use protocol::prompt::Prompt;

use super::super::OpenSession;
use super::{answered_simply, session, start_hub_answering};
use crate::Client;

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
