//! Checking a server: that it's asked, with what, and what's answered and logged. What's asked of one, and what's made
//! of what it does, is tested in the `tool-sign-in` crate, and whether there's anything to ask, in
//! `crate::tool_servers`.

use std::sync::Arc;
use std::time::Duration;

use logging::testing::Logs;
use protocol::client::tool_server_checks::{NoToolList, ToolServerCheck, ToolServerChecked, ToolServerToCheck};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey};
use tokio::task::JoinHandle;
use tool_sign_in::{Asks, Gate, Query, Script, StandIn};

use super::super::check_calls::MOST_CHECKING;
use super::support::{servers, told};
use crate::hub::Hub;
use crate::hub::tests::hub_in;

const KEY: &str = "made-up-key";

fn add(id: &str, name: &str, url: &str, auth: ToolServerAuthGiven) -> ToolServerAdd {
    ToolServerAdd { id: id.into(), name: name.into(), url: url.into(), auth }
}

fn with_key() -> ToolServerAuthGiven {
    ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(KEY) }
}

fn to_add(add: ToolServerAdd) -> ToolServerCheck {
    ToolServerCheck { of: ToolServerToCheck::Add(add) }
}

fn to_edit(url: &str, auth: Option<ToolServerAuthEdit>) -> ToolServerCheck {
    let edit = ToolServerEdit { id: "t".into(), name: None, url: Some(url.into()), auth };
    ToolServerCheck { of: ToolServerToCheck::Edit(edit) }
}

/// One anyone can use, whose URL has a query.
async fn open(query: &'static str) -> StandIn {
    StandIn::with(Script { asks: Asks::Nothing, query: Query::Repeated(query), ..Script::default() }).await
}

fn url(stand_in: &StandIn) -> String {
    stand_in.server().as_str().to_string()
}

/// Each with the key it's to have, and nothing of it, the URL's query or what the server said in the log.
#[tokio::test]
async fn a_server_is_asked_for_its_tools_before_its_added_or_edited_which_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), open("toolsets=made-up-set").await);
    let mut feed = hub.subscribe_feed();
    let logs = Logs::capture();

    let adding = hub.tool_server_check(to_add(add("new", "Linear", &url(&stand_in), with_key()))).await.unwrap();

    assert_eq!(adding, ToolServerChecked::Listed { tools: 2 });
    assert_eq!((servers(&hub), told(&mut feed)), (vec![], vec![]));
    assert!(!dir.path().join("tool-servers").exists(), "the key isn't kept");
    let added = hub.tool_server_add(add("t", "Linear", &url(&stand_in), with_key())).await.unwrap();
    told(&mut feed);
    let queried = url(&stand_in).replace("made-up-set", "other");
    stand_in.change(|script| script.asks = Asks::Answers(401));
    let editing = hub.tool_server_check(to_edit(&queried, None)).await.unwrap();

    assert_eq!(editing, ToolServerChecked::NotListed { why: NoToolList::KeyRefused { status: 401 } });
    assert_eq!((servers(&hub), told(&mut feed)), (vec![added], vec![]));
    let carried: Vec<_> = stand_in.asked().iter().map(|asked| asked.headers["authorization"].clone()).collect();
    assert_eq!(carried, vec![format!("Bearer {KEY}"); 3], "two as it's added, and one as it's edited");
    let logged = logs.text();
    let checked: Vec<_> = logged.lines().filter(|line| line.contains("tool_server.checked")).collect();
    assert_eq!(checked.len(), 2, "{logged}");
    assert!(!checked[0].contains("server_id"), "one that isn't the hub's has no ID of its own: {}", checked[0]);
    assert!(checked[1].contains(r#""server_id":"t""#) && checked[1].contains("KeyRefused"), "{}", checked[1]);
    for kept_out in [KEY, "made-up-set", "find_issue"] {
        assert!(!logged.contains(kept_out), "{logged}");
    }
}

#[tokio::test]
async fn what_cant_be_added_or_edited_is_refused_before_anything_is_asked_and_a_kept_key_goes_to_no_other_server() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, first, second) = (hub_in(dir.path()), open("a=1").await, open("a=1").await);
    hub.tool_server_add(add("t", "Linear", &url(&first), with_key())).await.unwrap();
    let keep = |given_for: &StandIn| {
        let (header, url) = ("Authorization".to_string(), url(given_for));
        Some(ToolServerAuthEdit::Key { header, key: None, url })
    };

    for (check, code) in [
        (to_add(add("new", "linear", &url(&second), with_key())), ErrorCode::Conflict),
        (to_edit(&url(&second), None), ErrorCode::InvalidRequest),
        (to_edit(&url(&second), keep(&second)), ErrorCode::InvalidRequest),
        (to_edit(&url(&second), keep(&first)), ErrorCode::Conflict),
    ] {
        assert_eq!(hub.tool_server_check(check).await.unwrap_err().code, code);
    }
    let named = ToolServerEdit { id: "t".into(), name: Some("Linear (Ada)".into()), url: None, auth: None };
    let nothing = hub.tool_server_check(ToolServerCheck { of: ToolServerToCheck::Edit(named) }).await.unwrap();

    assert_eq!(nothing, ToolServerChecked::NothingToAsk);
    assert!(first.asked().is_empty() && second.asked().is_empty());
}

fn open_to_add(stand_in: &StandIn) -> ToolServerCheck {
    to_add(add("t", "Linear", &url(stand_in), ToolServerAuthGiven::Neither))
}

/// As many checks of `held`, which holds its answers back, as there may be at once, each once it has been asked.
async fn as_many_as_may_be(hub: &Arc<Hub>, held: &StandIn) -> Vec<JoinHandle<Result<ToolServerChecked, ApiError>>> {
    let begin = |_| {
        let (hub, check) = (hub.clone(), open_to_add(held));
        tokio::spawn(async move { hub.tool_server_check(check).await })
    };
    let checks = (0..MOST_CHECKING).map(begin).collect();
    let all_asked = || held.asked().len() == MOST_CHECKING;
    waiting::task::until(Duration::from_secs(10), "they weren't asked", all_asked).await;
    checks
}

#[tokio::test]
async fn as_many_are_asked_at_once_as_may_be_and_one_thats_answered_makes_room() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::closed();
    let script = Script { asks: Asks::Nothing, server_after: Some(gate.clone()), ..Script::default() };
    let (hub, held, free) = (hub_in(dir.path()), StandIn::with(script).await, open("a=1").await);
    let checks = as_many_as_may_be(&hub, &held).await;

    let over = hub.tool_server_check(open_to_add(&free)).await.unwrap_err();
    gate.open();
    for held in checks {
        assert_eq!(held.await.unwrap().unwrap(), ToolServerChecked::Listed { tools: 2 });
    }

    assert_eq!(over.code, ErrorCode::Unavailable);
    assert!(free.asked().is_empty());
    assert_eq!(hub.tool_server_check(open_to_add(&free)).await.unwrap(), ToolServerChecked::Listed { tools: 2 });
}

/// The clock is moved once they've been asked, so nothing is waited out.
#[tokio::test]
async fn a_server_that_doesnt_answer_in_15_seconds_is_given_up_on_and_makes_room() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script { asks: Asks::Nothing, server_after: Some(Gate::closed()), ..Script::default() };
    let (hub, held, free) = (hub_in(dir.path()), StandIn::with(script).await, open("a=1").await);
    let checks = as_many_as_may_be(&hub, &held).await;

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(15)).await;
    tokio::time::resume();

    for given_up in checks {
        let given_up = tokio::time::timeout(Duration::from_secs(10), given_up).await.expect("it wasn't given up on");
        assert_eq!(given_up.unwrap().unwrap(), ToolServerChecked::NotListed { why: NoToolList::TimedOut });
    }
    assert_eq!(hub.tool_server_check(open_to_add(&free)).await.unwrap(), ToolServerChecked::Listed { tools: 2 });
}
