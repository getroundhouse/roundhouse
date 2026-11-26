//! Saving a server once it has listed its tools: what's saved, what's let go of while its server is asked, and what's
//! refused once it has been asked, when another client has changed it meanwhile. What a save that has been asked does
//! with each change meanwhile is tested in `crate::tool_servers`, and how many may be asked at once beside checks, in
//! `checking`.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::client::tool_server_checks::{NoToolList, ToolServerCheck, ToolServerChecked, ToolServerToCheck};
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSaved};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::model::{ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey};
use tool_sign_in::{Asks, Gate, Query, Script, StandIn};

use super::super::check_calls::MOST_CHECKING;
use super::support::{Saving, kept, save, servers, told};
use crate::hub::Hub;
use crate::hub::tests::hub_in;

fn url(stand_in: &StandIn) -> String {
    stand_in.server().as_str().to_string()
}

fn with_key(key: &str) -> ToolServerAuthGiven {
    ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(key) }
}

/// Having the server `t`, at `url`, know the person by `key`.
fn keyed(key: &str, url: &str) -> ToolServerEdit {
    let auth =
        ToolServerAuthEdit::Key { header: "Authorization".into(), key: Some(ToolServerKey::new(key)), url: url.into() };
    ToolServerEdit { id: "t".into(), name: None, url: None, auth: Some(auth), saved_anyway: false }
}

/// One anyone can use, whose URL has `query`, which holds its answers back until `gate` opens, if it's given one.
async fn open(query: &'static str, gate: Option<&Gate>) -> StandIn {
    let server_after = gate.cloned();
    StandIn::with(Script { asks: Asks::Nothing, query: Query::Repeated(query), server_after, ..Script::default() })
        .await
}

/// Adding `stand_in` as Docs, once it has listed its tools, as `id`.
fn docs(hub: &Arc<Hub>, id: &str, stand_in: &StandIn) -> Saving {
    let add = ToolServerAdd {
        id: "docs".into(),
        name: "Docs".into(),
        url: url(stand_in),
        auth: ToolServerAuthGiven::Neither,
    };
    save(hub, id, ToolServerToCheck::Add(add), SaveWhen::Listed).unwrap()
}

async fn saved(saving: Saving) -> ToolServerSaved {
    tokio::time::timeout(Duration::from_secs(10), saving.saved).await.expect("it wasn't answered").unwrap().unwrap()
}

#[tokio::test]
async fn a_server_is_saved_once_it_lists_its_tools_and_one_that_doesnt_only_anyway_without_asking_it() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script { asks: Asks::Nothing, query: Query::Repeated("a=1"), ..Script::default() };
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);
    let mut feed = hub.subscribe_feed();
    let add = ToolServerAdd { id: "t".into(), name: "Linear".into(), url: url(&stand_in), auth: with_key("first") };
    let another = || ToolServerToCheck::Edit(keyed("second", &url(&stand_in)));

    let added = saved(save(&hub, "a", ToolServerToCheck::Add(add), SaveWhen::Listed).unwrap()).await;
    stand_in.change(|script| script.asks = Asks::Answers(401));
    let refused = saved(save(&hub, "b", another(), SaveWhen::Listed).unwrap()).await;
    let asked = stand_in.asked().len();
    let as_it_was = (servers(&hub), told(&mut feed), kept(dir.path()));
    let anyway = saved(save(&hub, "c", another(), SaveWhen::Anyway).unwrap()).await;

    let [server] = as_it_was.0.as_slice() else { panic!("{:?}", as_it_was.0) };
    assert_eq!(added, ToolServerSaved::Saved { server: server.clone() });
    assert_eq!(as_it_was.1, std::slice::from_ref(server), "every client is told of it");
    assert_eq!(refused, ToolServerSaved::NotListed { why: NoToolList::KeyRefused { status: 401 } });
    assert_eq!(as_it_was.2["key"], "first", "the key it had");
    assert_eq!(anyway, ToolServerSaved::Saved { server: servers(&hub).remove(0) });
    assert_eq!((stand_in.asked().len(), kept(dir.path())["key"].as_str()), (asked, Some("second")));
}

/// Each of as many saves as may be asked at once: one cancelled, one whose connection closes, and two let be, which add
/// the same server. Nothing else lets go of one: not another ID, nor the server's own, which every client knows.
#[tokio::test]
async fn a_save_is_let_go_of_by_its_own_id_or_as_its_connection_closes_and_its_place_is_free_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::closed();
    let (hub, held, free) = (hub_in(dir.path()), open("a=1", Some(&gate)).await, open("a=1", None).await);
    let mut saves: Vec<Saving> = (0..MOST_CHECKING).map(|at| docs(&hub, &format!("s{at}"), &held)).collect();
    let all_asked = || held.asked().len() == MOST_CHECKING;
    waiting::task::until(Duration::from_secs(10), "they weren't asked", all_asked).await;
    let check = || {
        let add =
            ToolServerAdd { id: "c".into(), name: "C".into(), url: url(&free), auth: ToolServerAuthGiven::Neither };
        ToolServerCheck { of: ToolServerToCheck::Add(add) }
    };
    let (connection_goes, cancelled) = (saves.pop().unwrap(), saves.pop().unwrap());

    let again = save(&hub, "s0", ToolServerToCheck::Unknown, SaveWhen::Listed).err().map(|e| e.code);
    for not_its_own in ["docs", "s9"] {
        hub.tool_server_save_cancel(&not_its_own.into());
    }
    let over = hub.tool_server_check(check()).await.unwrap_err();
    hub.tool_server_save_cancel(&"s2".into());
    let let_go = saved(cancelled).await;
    let room = hub.tool_server_check(check()).await.unwrap();
    connection_goes.closing.send_replace(true);
    let gone = saved(connection_goes).await;
    gate.open();
    let mut added = Vec::new();
    for saving in saves {
        added.push(saved(saving).await);
    }
    hub.tool_server_save_cancel(&"s0".into());

    assert_eq!((again, over.code), (Some(ErrorCode::Conflict), ErrorCode::Unavailable));
    assert_eq!((let_go, gone), (ToolServerSaved::Cancelled, ToolServerSaved::Cancelled));
    assert_eq!(room, ToolServerChecked::Listed { tools: 2 });
    let [docs] = servers(&hub).try_into().unwrap();
    assert_eq!(added, vec![ToolServerSaved::Saved { server: docs.clone() }; 2]);
    assert_eq!(servers(&hub), [docs], "a cancel once it's saved is too late");
}

/// Another client gives it another key while it's asked at another query, which its key wasn't asked with there.
#[tokio::test]
async fn a_save_whose_server_is_given_another_key_while_its_asked_is_refused_and_the_other_change_stays() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::closed();
    let (hub, stand_in) = (hub_in(dir.path()), open("toolsets=one", Some(&gate)).await);
    let at = url(&stand_in);
    hub.tool_server_add(ToolServerAdd {
        id: "t".into(),
        name: "Linear".into(),
        url: at.clone(),
        auth: with_key("first"),
    })
    .await
    .unwrap();
    let queried = ToolServerEdit { url: Some(at.replace("one", "two")), auth: None, ..keyed("unused", &at) };
    let saving = save(&hub, "s", ToolServerToCheck::Edit(queried), SaveWhen::Listed).unwrap();
    waiting::task::until(Duration::from_secs(10), "it wasn't asked", || stand_in.asked().len() == 1).await;

    hub.tool_server_edit(keyed("second", &at)).await.unwrap();
    gate.open();
    let refused = tokio::time::timeout(Duration::from_secs(10), saving.saved).await.unwrap().unwrap().unwrap_err();

    assert_eq!(refused.code, ErrorCode::Conflict);
    assert_eq!((servers(&hub)[0].url.as_str(), kept(dir.path())["key"].as_str()), (at.as_str(), Some("second")));
}
