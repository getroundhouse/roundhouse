//! The tool servers a hub has of its own, as clients add and remove them on connections of their own. What's kept, in
//! what order, and what's refused is tested in `crate::tool_servers`.

use std::path::Path;
use std::sync::Arc;

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe, Subscription};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerRemove};
use protocol::client::{
    CLIENT_PROTOCOL_VERSION, ClientRequest, ClientResponse, HubFrame, HubNotice, ReplyOutcome, Request,
};
use protocol::model::{RecordKey, ToolServer, ToolServerAuthGiven, ToolServerKey};

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
async fn a_server_one_client_adds_and_removes_reaches_another() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut adds = client(&hub).await;
    let (mut second, listed) = following(&hub).await;

    let added = ask(&mut adds, 2, add("t", "Linear", a_key())).await;
    told(&mut second, &HubNotice::ToolServer(added.clone())).await;
    let removed = ask(&mut adds, 3, ToolServerRemove { id: "t".into() }).await;
    second
        .until("its removal", |sent| {
            let gone = RecordKey::ToolServer { id: "t".into() };
            sent.iter().any(|frame| {
                matches!(frame, HubFrame::Notice { notice: HubNotice::Removed { record, .. }, .. } if *record == gone)
            })
        })
        .await;

    assert_eq!(listed, []);
    assert_eq!(removed, added);
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
