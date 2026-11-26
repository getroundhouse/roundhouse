use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaved};
use protocol::client::tool_sign_ins::{
    ToolSignInBegun, ToolSignInFinish, ToolSignInPage, ToolSignInReturned, ToolSignInStart, ToolSignInTo, redirect,
};
use protocol::client::{ApiError, HubNotice};
use protocol::model::{ToolServer, ToolServerAuth, ToolSignInStatus};
use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tool_sign_in::{AuthorizationPage, StandIn};

use crate::hub::{Hub, Notices};

/// Signing in, as `id`, to a new server `t`, called `Linear`, at `stand_in`'s address.
pub(super) fn new(id: &str, stand_in: &StandIn) -> ToolSignInStart {
    let url = stand_in.server().as_str().to_string();
    start(id, ToolSignInTo::New { id: "t".into(), name: "Linear".into(), url })
}

pub(super) fn start(id: &str, to: ToolSignInTo) -> ToolSignInStart {
    ToolSignInStart { id: id.into(), to, redirect: redirect(17867) }
}

/// The page that starting `asked` is answered with.
pub(super) async fn page(hub: &Hub, asked: ToolSignInStart) -> ToolSignInPage {
    let begun = hub.tool_sign_in_start(asked).await.unwrap();
    let ToolSignInBegun::Page { page } = begun else { panic!("{begun:?}") };
    page
}

/// Finishing `id` with what `stand_in` sends the person back with from `page`.
pub(super) fn visited(id: &str, stand_in: &StandIn, page: &ToolSignInPage) -> ToolSignInFinish {
    finish(id, &stand_in.visit(&AuthorizationPage::new(page.as_str())))
}

pub(super) fn finish(id: &str, returned: &str) -> ToolSignInFinish {
    ToolSignInFinish { id: id.into(), returned: ToolSignInReturned::new(returned) }
}

/// Starts `asked`, has `stand_in`'s person do on its page what they do, and finishes it.
pub(super) async fn finished(hub: &Hub, stand_in: &StandIn, asked: ToolSignInStart) -> Result<ToolServer, ApiError> {
    let id = asked.id.clone();
    let page = page(hub, asked).await;
    hub.tool_sign_in_finish(visited(id.as_str(), stand_in, &page)).await
}

/// The server `t`, signed in to at `stand_in`.
pub(super) async fn signed_in(hub: &Hub, stand_in: &StandIn) -> ToolServer {
    finished(hub, stand_in, new("i", stand_in)).await.unwrap()
}

pub(super) fn servers(hub: &Hub) -> Vec<ToolServer> {
    hub.store.list_records().unwrap().tool_servers
}

pub(super) const WORKS: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::Works };
pub(super) const NEEDS_SIGN_IN: ToolServerAuth = ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsSignIn };

/// What's in the file of the one server that has one, in the hub's data directory `dir`.
pub(super) fn kept(dir: &Path) -> Value {
    let mut entries = std::fs::read_dir(dir.join("tool-servers")).unwrap();
    let text = std::fs::read_to_string(entries.next().unwrap().unwrap().path()).unwrap();
    assert!(entries.next().is_none(), "only one");
    serde_json::from_str(&text).unwrap()
}

/// The access token it holds, if it holds one.
pub(super) fn access(dir: &Path) -> Option<String> {
    kept(dir)["tokens"]["access"].as_str().map(str::to_string)
}

/// Each record of a tool server `feed` has told of so far.
pub(super) fn told(feed: &mut Notices) -> Vec<ToolServer> {
    let notices = std::iter::from_fn(|| feed.try_recv().ok().map(Arc::unwrap_or_clone));
    let of_server = |notice| if let HubNotice::ToolServer(server) = notice { Some(server) } else { None };
    notices.filter_map(of_server).collect()
}

/// Once `stand_in` has been asked for tokens `times`.
pub(super) async fn until_asked_for_tokens(stand_in: &StandIn, times: usize) {
    let asked = || stand_in.asked_for("POST /token").len() == times;
    waiting::task::until(Duration::from_secs(10), "its tokens weren't asked for", asked).await;
}

/// A save under way, on a task of its own, and what closes its connection.
pub(super) struct Saving {
    pub(super) closing: watch::Sender<bool>,
    pub(super) saved: JoinHandle<Result<ToolServerSaved, ApiError>>,
}

/// Saving `of`, as `id`, `when` it says, on a connection of its own, or why it's refused at once.
pub(super) fn save(hub: &Arc<Hub>, id: &str, of: ToolServerToCheck, when: SaveWhen) -> Result<Saving, ApiError> {
    let (closing, closed) = watch::channel(false);
    let saving = hub.tool_server_save(ToolServerSave { id: id.into(), of, when }, closed)?;
    Ok(Saving { closing, saved: tokio::spawn(saving) })
}
