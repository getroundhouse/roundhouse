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

/// Nothing cancels the sign-in of an app that quits, so one more than may wait forgets the one that has waited longest,
/// rather than having everyone wait until it has waited as long as one does.
#[test]
fn as_many_wait_as_may_and_one_more_forgets_the_one_that_has_waited_longest() {
    let waiting = Waiting::default();
    let begun: Vec<_> = (0..MOST_WAITING).map(|number| waiting.begin(&id(number), to(), 0).unwrap()).collect();
    let late_ms = later_ms(0, SIGN_IN_WAITS);

    let first = waiting.begin(&id(0), to(), 1).unwrap();
    assert!(Arc::ptr_eq(&first, &begun[0]), "one that waits is begun again as it is");
    waiting.cancel(&id(0));
    assert!(!waiting.holds(&id(0), &first) && !waiting.keep(&id(0), &first));
    waiting.begin(&id(MOST_WAITING), to(), 1).unwrap();
    assert!((1..=MOST_WAITING).all(|number| waiting.get(&id(number), 1).is_some()), "a cancelled one made room");
    let more = waiting.begin(&id(MOST_WAITING + 1), to(), 2).unwrap();
    assert!(!waiting.holds(&id(1), &begun[1]), "the first of those that began together is forgotten");
    assert!(waiting.holds(&id(2), &begun[2]) && waiting.holds(&id(MOST_WAITING + 1), &more));
    let second = waiting.get(&id(2), late_ms - 1).unwrap();
    waiting.end(&id(2), &second);
    assert!(waiting.get(&id(2), late_ms - 1).is_none());
    assert!(waiting.get(&id(3), late_ms - 1).is_some());
    assert!(waiting.get(&id(3), late_ms).is_none());
    assert!(waiting.get(&id(MOST_WAITING), late_ms).is_some(), "it waits from when it was started");
}

/// Which is for moments, so a start is refused rather than have it forgotten as its server is told.
#[test]
fn one_more_than_may_wait_is_refused_while_each_that_waits_is_being_kept() {
    let waiting = Waiting::default();
    let last = MOST_WAITING - 1;
    for number in 0..MOST_WAITING {
        let sign_in = waiting.begin(&id(number), to(), 0).unwrap();
        assert!(number == last || waiting.keep(&id(number), &sign_in));
    }

    let more = waiting.begin(&id(MOST_WAITING), to(), 1).unwrap();
    assert!(waiting.keep(&id(MOST_WAITING), &more));
    let over = waiting.begin(&id(MOST_WAITING + 1), to(), 1).map(|_| ());

    assert!(
        waiting.get(&id(last), 1).is_none(),
        "the one that wasn't being kept is forgotten, of those that began first"
    );
    assert_eq!(over.unwrap_err().code, ErrorCode::Unavailable);
    assert!((0..last).chain([MOST_WAITING]).all(|number| waiting.get(&id(number), 1).is_some()));
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
