//! How long a sign-in waits, and how many do, by times made up.

use std::sync::Arc;

use protocol::client::ErrorCode;
use protocol::client::tool_sign_ins::{SIGN_IN_WAITS, ToolSignInTo};
use protocol::model::ToolSignInId;

use super::super::waiting::{MOST_WAITING, Waiting};
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
fn as_many_wait_as_may_and_one_that_ends_is_cancelled_or_has_waited_long_enough_makes_room() {
    let waiting = Waiting::default();
    for number in 0..MOST_WAITING {
        waiting.begin(&id(number), to(), 0).unwrap();
    }
    let over = || waiting.begin(&id(MOST_WAITING), to(), 1).map(|_| ());
    let late_ms = later_ms(0, SIGN_IN_WAITS);

    assert_eq!(over().unwrap_err().code, ErrorCode::Unavailable);
    let first = waiting.begin(&id(0), to(), 1).unwrap();
    assert!(waiting.holds(&id(0), &first), "one that waits is begun again as it is");
    waiting.cancel(&id(0));
    assert!(!waiting.holds(&id(0), &first) && !waiting.keep(&id(0), &first));
    over().unwrap();
    let second = waiting.get(&id(1), late_ms - 1).unwrap();
    waiting.end(&id(1), &second);
    assert!(waiting.get(&id(1), late_ms - 1).is_none());
    assert!(waiting.get(&id(2), late_ms - 1).is_some());
    assert!(waiting.get(&id(2), late_ms).is_none());
    assert!(waiting.get(&id(MOST_WAITING), late_ms).is_some(), "it waits from when it was started");
}

#[test]
fn one_being_kept_is_neither_cancelled_nor_forgotten_until_it_ends() {
    let waiting = Waiting::default();
    let sign_in = waiting.begin(&id(0), to(), 0).unwrap();
    assert!(waiting.keep(&id(0), &sign_in));

    waiting.cancel(&id(0));

    assert!(waiting.get(&id(0), later_ms(0, SIGN_IN_WAITS)).is_some());
    waiting.end(&id(0), &sign_in);
    assert!(waiting.get(&id(0), 0).is_none());
}
