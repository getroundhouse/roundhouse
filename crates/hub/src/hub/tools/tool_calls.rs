use super::super::{Hub, new_id};
use crate::worker_link::WorkerLink;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{SessionId, WorkspaceId};
use protocol::worker::{
    AskReply, CancelWait, CreateWorktree, DiffComments, PullRequestAnswer, ReadWait, RemoveWorktree, ServeTools,
    ToolsServing, ToolsToken, WorkerAsk,
};
use std::sync::Arc;
use std::time::Duration;

/// A worker on this machine starts serving in milliseconds. Every wait on another process has a limit, as
/// `docs/principles/failure.md` says, so a worker slow to serve fails the launch it was asked for, and the next launch
/// asks it again.
const SERVE_TIMEOUT: Duration = Duration::from_secs(10);

impl Hub {
    /// What the session's agent reaches the worker's tools with, once the worker serves them: on the port they were
    /// last on, or on another it records when something else has taken that, with the token the session has had since
    /// its first. An agent never starts without them, so a worker that doesn't serve them, in time or at all, fails the
    /// launch, and nothing is kept of it for the next.
    pub(crate) async fn tools_for(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
        link: &WorkerLink,
    ) -> Result<ToolsToken, ApiError> {
        let id = workspace_id.clone();
        let recorded = self.blocking(move |store| store.tools_port(&id)).await?;
        let serving = link.call(ServeTools { port: recorded });
        let ToolsServing { port } = match tokio::time::timeout(SERVE_TIMEOUT, serving).await {
            Ok(served) => served.map_err(|e| unserved(&error_chain(&e)))?,
            Err(_) => return Err(unserved(&format!("it didn't answer within {SERVE_TIMEOUT:?}"))),
        };
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
                let pull_request = self.pull_request_for_agent(workspace_id).await?;
                Ok(AskReply::PullRequest(PullRequestAnswer { pull_request }))
            }
            WorkerAsk::PullRequests(asked) => {
                self.remote_for_agent(workspace_id, asked).await.map(AskReply::PullRequests)
            }
            WorkerAsk::StartPullRequestWait(asked) => {
                self.start_remote_wait(workspace_id, session_id, asked).await.map(AskReply::Wait)
            }
            WorkerAsk::Wait(ReadWait { wait }) => self.wait_of(&session_id, wait).await.map(AskReply::Wait),
            WorkerAsk::CancelWait(CancelWait { wait }) => self.cancel_wait(&session_id, wait).await.map(AskReply::Wait),
            WorkerAsk::CreateWorktree(CreateWorktree { on, key }) => {
                self.create_child(workspace_id, link, session_id, on, key).await.map(AskReply::WorktreeCreated)
            }
            WorkerAsk::RemoveWorktree(RemoveWorktree { path, force }) => {
                self.remove_child(workspace_id, link, path, force).await.map(AskReply::from)
            }
            WorkerAsk::ListWorktrees => self.list_worktrees(workspace_id, link).await.map(AskReply::Worktrees),
            WorkerAsk::Propose(asked) => self.propose(workspace_id, session_id, asked).await.map(AskReply::Proposal),
            WorkerAsk::Revise(asked) => self.revise(&session_id, asked).await.map(AskReply::Proposal),
            WorkerAsk::Withdraw(asked) => self.withdraw(&session_id, asked).await.map(AskReply::Proposal),
            WorkerAsk::ReadProposal(asked) => {
                self.read_proposal_revision(&session_id, asked).await.map(AskReply::Proposal)
            }
            WorkerAsk::DiffComments => {
                let comments = self.comment_list(workspace_id.clone()).await?;
                Ok(AskReply::DiffComments(DiffComments { comments }))
            }
            WorkerAsk::RenewToolServer(asked) => self.tool_server_refused(asked).await.map(AskReply::ToolServer),
            WorkerAsk::Unknown => Err(self.unsupported(None)),
        }
    }
}

/// Why an agent didn't start: its worker didn't serve Roundhouse's tools, because of `why`.
fn unserved(why: &str) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("the worker didn't serve Roundhouse's tools: {why}"))
}
