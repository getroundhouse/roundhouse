//! The tool servers a hub has of its own, as clients check, add and remove them on connections of their own. What's
//! kept, in what order, and what's refused is tested in `crate::tool_servers`.

use std::path::Path;
use std::sync::Arc;

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe, Subscription};
use protocol::client::tool_server_checks::{ToolServerCheck, ToolServerChecked, ToolServerToCheck};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit, ToolServerRemove};
use protocol::client::tool_sign_ins::{
    REDIRECT_PORTS, ToolSignInBegun, ToolSignInCancel, ToolSignInFinish, ToolSignInReturned, ToolSignInStart,
    ToolSignInTo, redirect,
};
use protocol::client::{
    CLIENT_PROTOCOL_VERSION, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, ReplyOutcome, Request,
};
use protocol::model::{RecordKey, ToolServer, ToolServerAuth, ToolServerAuthGiven, ToolServerKey, ToolSignInStatus};
use tool_sign_in::{Asks, AuthorizationPage, Script, StandIn};

use super::Connection;
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};

async fn client(hub: &Arc<Hub>) -> Connection {
    Connection::greeted(hub, CLIENT_PROTOCOL_VERSION).await
}

/// One that follows the hub, and the tool servers it was listed.
async fn following(hub: &Arc<Hub>) -> (Connection, Vec<ToolServer>) {
    let mut client = client(hub).await;
    let start = vec![Subscription::new(1, Follow::Hub)];
    client.reply(1, &ClientRequest::Subscribe(Subscribe { stop: Vec::new(), start })).await;
    let list = |frame: &HubFrame| {
        let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame else { return None };
        Some(records.tool_servers.clone())
    };
    client.until("its list", |sent| sent.iter().any(|frame| list(frame).is_some())).await;
    let listed = client.sent.iter().find_map(list).unwrap();
    (client, listed)
}

/// What the hub tells `client` of it, `notice`, among whatever else it sends.
async fn told(client: &mut Connection, notice: &HubNotice) {
    let told = |frame: &HubFrame| matches!(frame, HubFrame::Notice { notice: told, .. } if told == notice);
    client.until("the notice", |sent| sent.iter().any(told)).await;
}

async fn ask(client: &mut Connection, id: u64, request: impl Request) -> ToolServer {
    let reply = client.reply(id, &request.into_request()).await;
    let ReplyOutcome::Response(ClientResponse::ToolServer(server)) = reply else { panic!("{reply:?}") };
    server
}

fn add(id: &str, name: &str, auth: ToolServerAuthGiven) -> ToolServerAdd {
    ToolServerAdd { id: id.into(), name: name.into(), url: "https://mcp.example.com/mcp".into(), auth }
}

fn a_key() -> ToolServerAuthGiven {
    ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new("made-up") }
}

fn files_in(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("tool-servers")).unwrap().count()
}

#[tokio::test]
async fn a_server_one_client_adds_edits_and_removes_reaches_another() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut adds = client(&hub).await;
    let (mut second, listed) = following(&hub).await;

    let added = ask(&mut adds, 2, add("t", "Linear", a_key())).await;
    told(&mut second, &HubNotice::ToolServer(added.clone())).await;
    let rename =
        ToolServerEdit { id: "t".into(), name: Some("Sentry".into()), url: None, auth: None, saved_anyway: false };
    let edited = ask(&mut adds, 3, rename).await;
    told(&mut second, &HubNotice::ToolServer(edited.clone())).await;
    let removed = ask(&mut adds, 4, ToolServerRemove { id: "t".into() }).await;
    second
        .until("its removal", |sent| {
            let gone = RecordKey::ToolServer { id: "t".into() };
            sent.iter().any(|frame| {
                matches!(frame, HubFrame::Notice { notice: HubNotice::Removed { record, .. }, .. } if *record == gone)
            })
        })
        .await;

    assert_eq!(listed, []);
    assert_eq!(edited.name.as_str(), "Sentry");
    assert_eq!(removed, edited);
}

/// What's asked, and what's made of the answer, is tested in `crate::hub` and the `tool-sign-in` crate.
#[tokio::test]
async fn a_client_is_answered_with_what_a_check_comes_to_and_nothing_is_added() {
    let dir = tempfile::tempdir().unwrap();
    let script = Script { asks: Asks::Nothing, ..Script::default() };
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(script).await);
    let mut client = client(&hub).await;
    let url = stand_in.server().as_str().to_string();
    let of = ToolServerToCheck::Add(ToolServerAdd { url, ..add("t", "Linear", a_key()) });

    let reply = client.reply(2, &ToolServerCheck { of }.into_request()).await;

    let ReplyOutcome::Response(ClientResponse::ToolServerChecked(checked)) = reply else { panic!("{reply:?}") };
    assert_eq!(checked, ToolServerChecked::Listed { tools: 2 });
    assert_eq!(following(&hub).await.1, []);
}

/// How it's waited for, finished and refused is tested in `crate::hub`.
#[tokio::test]
async fn a_server_one_client_signs_in_to_reaches_another_and_a_cancelled_sign_in_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, stand_in) = (hub_in(dir.path()), StandIn::with(Script::default()).await);
    let mut signs_in = client(&hub).await;
    let (mut second, _) = following(&hub).await;
    let start = |id: &str, server: &str| {
        let url = stand_in.server().as_str().to_string();
        let to = ToolSignInTo::New { id: server.into(), name: server.into(), url };
        ToolSignInStart { id: id.into(), to, redirect: redirect(REDIRECT_PORTS[0]) }
    };

    let begun = signs_in.reply(2, &start("i", "linear").into_request()).await;
    let ReplyOutcome::Response(ClientResponse::ToolSignInBegun(ToolSignInBegun::Page { page })) = begun else {
        panic!("{begun:?}")
    };
    let returned = ToolSignInReturned::new(stand_in.visit(&AuthorizationPage::new(page.as_str())));
    let server = ask(&mut signs_in, 3, ToolSignInFinish { id: "i".into(), returned: returned.clone() }).await;
    told(&mut second, &HubNotice::ToolServer(server.clone())).await;
    signs_in.reply(4, &start("cancelled", "docs").into_request()).await;
    let cancelled = signs_in.reply(5, &ToolSignInCancel { id: "cancelled".into() }.into_request()).await;
    let finished = signs_in.reply(6, &ToolSignInFinish { id: "cancelled".into(), returned }.into_request()).await;

    assert_eq!(server.auth, ToolServerAuth::SignIn { status: ToolSignInStatus::Works });
    assert!(matches!(cancelled, ReplyOutcome::Response(ClientResponse::Ok)), "{cancelled:?}");
    assert!(matches!(&finished, ReplyOutcome::Error(e) if e.code == ErrorCode::NotFound), "{finished:?}");
}

#[tokio::test]
async fn servers_are_there_after_the_hub_restarts_with_a_key_and_with_neither() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut adds = client(&hub).await;
    let key = ask(&mut adds, 2, add("k", "Linear", a_key())).await;
    let neither = ask(&mut adds, 3, add("n", "Docs", ToolServerAuthGiven::Neither)).await;
    drop((adds, hub));

    let again = hub_in_again(dir.path());
    // As a hub does as it starts.
    again.tidy_logins().await;
    let (_, listed) = following(&again).await;

    assert_eq!(listed, [neither, key]);
    assert_eq!(files_in(dir.path()), 1, "its key is there too");
}

/// And what one added as the hub stopped left, a file no server names, goes as the next starts.
#[tokio::test]
async fn a_server_removed_leaves_no_key_behind_and_neither_does_one_the_hub_stopped_adding() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut client = client(&hub).await;
    ask(&mut client, 2, add("k", "Linear", a_key())).await;
    let kept = files_in(dir.path());

    ask(&mut client, 3, ToolServerRemove { id: "k".into() }).await;
    let left = files_in(dir.path());
    drop((client, hub));
    let stray = dir.path().join("tool-servers").join(crate::login_files::LoginFile::new().as_str());
    std::fs::write(stray, r#"{"format":"key","key":"made-up"}"#).unwrap();
    let again = hub_in_again(dir.path());
    again.tidy_logins().await;
    let (_, listed) = following(&again).await;

    assert_eq!((kept, left, files_in(dir.path())), (1, 0, 0));
    assert_eq!(listed, []);
}
