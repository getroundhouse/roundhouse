//! Answering a request once a connection is open: one short arm per request, each calling one operation.

use std::sync::Arc;

use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode};
use tokio::task::AbortHandle;

use super::Connection;
use super::pump::{pump, pump_feed};
use crate::hub::Hub;

/// Holds the lock while `start` runs, so two requests can't both take one subscription ID.
fn add_subscription(
    connection: &Connection,
    subscription_id: u64,
    start: impl FnOnce() -> AbortHandle,
) -> Result<(), ApiError> {
    let mut subscriptions = connection.subscriptions.lock();
    if subscriptions.contains_key(&subscription_id) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("subscription {subscription_id} already exists")));
    }
    subscriptions.insert(subscription_id, start());
    Ok(())
}

/// Gives up waiting once the connection closes, since nobody is left to tell, and an open request keeps the
/// connection counted among the hub's clients.
async fn until_closed<T>(
    connection: &Connection,
    waiting: impl Future<Output = Result<T, ApiError>>,
) -> Result<T, ApiError> {
    let mut closed = connection.closed.subscribe();
    tokio::select! {
        result = waiting => result,
        _ = closed.wait_for(|closed| *closed) => Err(ApiError::new(ErrorCode::Conflict, "the connection closed")),
    }
}

#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
pub(super) async fn handle(
    hub: &Arc<Hub>,
    connection: &Arc<Connection>,
    request: ClientRequest,
) -> Result<ClientResponse, ApiError> {
    use ClientRequest as R;
    use ClientResponse as C;
    Ok(match request {
        R::Hello { .. } => return Err(ApiError::new(ErrorCode::InvalidRequest, "already said hello")),
        R::Pair { .. } | R::Authenticate { .. } => {
            let message = "only the first request on a connection from the network can pair or authenticate";
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        R::Unknown => {
            let message = format!("this hub, version {}, doesn't know that request", protocol::BUILD_VERSION);
            return Err(ApiError::new(ErrorCode::Unsupported, message));
        }
        R::HubStatus => C::HubStatus(hub.status()),
        R::HubShutdown => {
            hub.request_shutdown();
            C::Ok
        }
        R::PairingStart => C::Pairing(hub.pairing_start().await?),
        R::PairingWait { code } => C::Device(until_closed(connection, hub.pairing_wait(code)).await?),
        R::DeviceList => C::Devices { devices: hub.device_list().await? },
        R::DeviceRevoke { id } => {
            hub.device_revoke(id).await?;
            C::Ok
        }
        R::RepoAdd { path } => C::Repo(hub.repo_add(path).await?),
        R::RepoList => C::Repos { repos: hub.repo_list().await? },
        R::RecordsList => C::Records(hub.records_list().await?),
        R::WorkspaceCreate { repo_id, name, base_ref, client_request_id } => {
            C::Workspace(hub.workspace_create(repo_id, name, base_ref, client_request_id).await?)
        }
        R::WorkspaceList { include_archived } => {
            C::Workspaces { workspaces: hub.workspace_list(include_archived).await? }
        }
        R::WorkspaceArchive { id, force } => C::Workspace(hub.workspace_archive(id, force).await?),
        R::WorkspaceRestore { id } => C::Workspace(hub.workspace_restore(id).await?),
        R::WorkspaceStatus { id } => C::GitStatus(hub.workspace_status(id).await?),
        R::WorkspaceDiff { id } => C::GitDiff(hub.workspace_diff(id).await?),
        R::WorkspaceSetupLog { id } => C::SetupLog { text: hub.workspace_setup_log(id).await? },
        R::WorkspaceRerunSetup { id } => C::Workspace(hub.workspace_rerun_setup(id).await?),
        R::WorkspacePullRequest { id } => C::PullRequest { pull_request: hub.workspace_pull_request(id).await? },
        R::CommentAdd { workspace_id, path, side, line, text, client_request_id } => {
            C::Comment(hub.comment_add(workspace_id, path, side, line, text, client_request_id).await?)
        }
        R::CommentList { workspace_id } => C::Comments { comments: hub.comment_list(workspace_id).await? },
        R::CommentDelete { id } => {
            hub.comment_delete(id).await?;
            C::Ok
        }
        R::SessionCreate { workspace_id, agent, model, permission_mode, client_request_id } => {
            C::Session(hub.session_create(workspace_id, agent, model, permission_mode, client_request_id).await?)
        }
        R::SessionEnsure { workspace_id, agent } => C::Session(hub.session_ensure(workspace_id, agent).await?),
        R::SessionList { workspace_id } => C::Sessions { sessions: hub.session_list(workspace_id).await? },
        R::SessionGet { id } => C::Session(hub.session_get(id).await?),
        R::SessionRead { id, from_seq, limit, max_bytes, include_raw } => {
            hub.session_read(id, from_seq, limit, max_bytes, include_raw).await?
        }
        R::SessionSubscribe { id, from_seq, subscription_id } => {
            let session = hub.live_session(&id).await?;
            add_subscription(connection, subscription_id, || {
                let live = session.subscribe();
                let frames = connection.frames.clone();
                tokio::spawn(pump(hub.clone(), session.clone(), id, subscription_id, from_seq, live, frames))
                    .abort_handle()
            })?;
            C::Ok
        }
        R::FeedSubscribe { subscription_id } => {
            add_subscription(connection, subscription_id, || {
                tokio::spawn(pump_feed(hub.subscribe_feed(), subscription_id, connection.frames.clone())).abort_handle()
            })?;
            C::Ok
        }
        R::SessionUnsubscribe { subscription_id } => {
            if let Some(subscription) = connection.subscriptions.lock().remove(&subscription_id) {
                subscription.abort();
            }
            C::Ok
        }
        R::SessionPrompt { id, text, client_msg_id } => {
            let (seq, duplicate) = hub.session_prompt(id, text, client_msg_id).await?;
            C::PromptRecorded { seq, duplicate }
        }
        R::SessionApprove { id, approval_id, decision } => {
            hub.session_approve(id, approval_id, decision).await?;
            C::Ok
        }
        R::SessionConfigure { id, model, permission_mode } => {
            C::Session(hub.session_configure(id, model, permission_mode).await?)
        }
        R::SessionClose { id } => C::Session(hub.session_close(id).await?),
        R::SessionReopen { id } => C::Session(hub.session_reopen(id).await?),
        R::SessionCancel { id } => {
            hub.session_cancel(id).await?;
            C::Ok
        }
    })
}
