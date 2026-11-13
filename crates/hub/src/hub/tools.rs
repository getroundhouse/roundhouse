//! The worker's tools, which its agents call, and what the hub answers for them. A worker asks only for its own
//! workspace, which the hub knows, so a tool reaches that workspace and nothing else.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{SessionId, WorkspaceId};
use protocol::worker::{
    AskReply, CancelWait, PullRequestAnswer, ReadWait, ServeTools, StartWait, ToolsServing, ToolsToken, WorkerAsk,
};
use tracing::warn;

use super::{Hub, new_id};
use crate::worker_link::WorkerLink;

#[cfg(test)]
mod tests;

/// A worker on this machine starts serving in milliseconds, and one that doesn't know the request never answers it, so
/// an agent starts without the tools rather than waiting, and so do the worker's agents after it.
const SERVE_TIMEOUT: Duration = Duration::from_secs(10);

impl Hub {
    /// What the session's agent reaches the worker's tools with, once the worker serves them. `None` if it can't, and
    /// the agent goes without them.
    pub(crate) async fn tools_for(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
        link: &WorkerLink,
    ) -> Option<ToolsToken> {
        match self.serve_tools(workspace_id, session_id, link).await {
            Ok(token) => Some(token),
            Err(e) => {
                warn!(%workspace_id, "the agent goes without the worker's tools: {}", error_chain(&e));
                None
            }
        }
    }

    /// Has the worker serve its tools on the port they were last on, which it can't when something else has taken it,
    /// so the port they're on is recorded, and gives the session the token it has had since its first.
    async fn serve_tools(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
        link: &WorkerLink,
    ) -> Result<ToolsToken, ApiError> {
        let unserved =
            || ApiError::new(ErrorCode::Worker, format!("the worker didn't serve its tools within {SERVE_TIMEOUT:?}"));
        if link.serves_no_tools() {
            return Err(unserved());
        }
        let id = workspace_id.clone();
        let recorded = self.blocking(move |store| store.tools_port(&id)).await?;
        let serving = link.call(ServeTools { port: recorded });
        let ToolsServing { port } = tokio::time::timeout(SERVE_TIMEOUT, serving).await.map_err(|_| {
            link.tools_unserved();
            unserved()
        })??;
        if recorded != Some(port) {
            let id = workspace_id.clone();
            self.blocking(move |store| store.set_tools_port(&id, port)).await?;
        }
        let (id, made) = (session_id.clone(), ToolsToken::new(new_id()));
        self.blocking(move |store| store.tools_token(&id, &made)).await
    }

    /// Answers what the worker on `link`, for `workspace_id`, asked for the agent in `session_id`, which has to be one
    /// of that workspace's. Through that worker, since it's the one that asked, so answering never starts one.
    pub(crate) async fn answer_ask(
        self: &Arc<Self>,
        workspace_id: &WorkspaceId,
        link: &WorkerLink,
        session_id: SessionId,
        ask: WorkerAsk,
    ) -> Result<AskReply, ApiError> {
        if self.session_get(session_id.clone()).await?.workspace_id != *workspace_id {
            return Err(ApiError::new(ErrorCode::NotFound, "that chat isn't in this workspace"));
        }
        match ask {
            WorkerAsk::PullRequest => {
                let pull_request = self.pull_request_for_agent(workspace_id, link).await?;
                Ok(AskReply::PullRequest(PullRequestAnswer { pull_request }))
            }
            WorkerAsk::StartWait(StartWait { until, call }) => {
                self.start_wait(workspace_id, link, session_id, until, call).await.map(AskReply::Wait)
            }
            WorkerAsk::Wait(ReadWait { wait }) => self.wait_of(&session_id, wait).await.map(AskReply::Wait),
            WorkerAsk::CancelWait(CancelWait { wait }) => self.cancel_wait(&session_id, wait).await.map(AskReply::Wait),
            WorkerAsk::Unknown => Err(self.unsupported(None)),
        }
    }
}
