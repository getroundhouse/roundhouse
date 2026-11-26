//! How long a sign-in waits, and what else of its ID does meanwhile, by times made up.

use std::sync::Arc;

use protocol::client::ErrorCode;
use protocol::client::tool_sign_ins::{SIGN_IN_WAITS, ToolSignInTo};
use protocol::model::ToolSignInId;

use super::super::waiting::Waiting;
use crate::store::later_ms;
use crate::store::tests::open_in;
use crate::tool_servers::{SigningIn, ToolServers};

/// What one is to, which is nothing to how long it waits.
fn to() -> SigningIn {
    let dir = tempfile::tempdir().unwrap();
    let servers = ToolServers::new(Arc::new(open_in(dir.path())), dir.path());
    let url = "https://mcp.example.com/mcp".to_string();
    servers.signing_in(ToolSignInTo::New { id: "t".into(), name: "Linear".into(), url }).unwrap()
}

fn id(number: usize) -> ToolSignInId {
    format!("i-{number}").into()
}

#[test]
fn one_waits_from_when_it_began_and_another_of_its_id_is_refused_meanwhile() {
    let waiting = Waiting::default();
    let late_ms = later_ms(0, SIGN_IN_WAITS);
    let first = waiting.begin(&id(0), to(), 0).unwrap();
    let other = waiting.begin(&id(1), to(), 1).unwrap();

    let again = waiting.begin(&id(0), to(), late_ms - 1).map(|_| ());
    let waits_until = (waiting.get(&id(0), late_ms - 1).is_some(), waiting.get(&id(0), late_ms).is_none());
    let after = waiting.begin(&id(0), to(), late_ms).unwrap();
    waiting.end(&id(0), &first);
    waiting.cancel(&id(1));

    assert_eq!(again.unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(waits_until, (true, true));
    assert!(waiting.holds(&id(0), &after), "ending the one before forgets only that one");
    assert!(!waiting.holds(&id(1), &other), "it waited from when it was started, until it was cancelled");
}
