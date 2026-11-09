//! What workspaces and chats are called: titled from the first prompt, then by a generated title, or renamed by hand.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AgentKind, Session, SessionId, Title, Workspace, WorkspaceId};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tracing::warn;

use super::{Hub, unexpected};
use crate::store::Retitle;

/// Enough of a prompt to say what it's about. Past this it's usually a pasted log, which only costs more to send and
/// to ask about.
const MOST_PROMPT_CHARS: usize = 4000;

impl Hub {
    pub(crate) async fn workspace_rename(&self, id: WorkspaceId, title: &str) -> Result<Workspace, ApiError> {
        let title = Title::typed(title)?;
        let renamed = self.blocking(move |store| store.retitle_workspace(&id, &title, Retitle::ByHand)).await?.record;
        self.notify(HubNotice::Workspace(renamed.clone()));
        Ok(renamed)
    }

    pub(crate) async fn session_rename(&self, id: SessionId, title: &str) -> Result<Session, ApiError> {
        let title = Title::typed(title)?;
        let renamed = self.blocking(move |store| store.retitle_session(&id, &title, Retitle::ByHand)).await?.record;
        self.notify(HubNotice::Session(renamed.clone()));
        Ok(renamed)
    }

    /// Titles a chat from a prompt just recorded in it, if the chat has no title yet, and its workspace too on the
    /// same terms. Only a workspace none of whose chats has had a prompt is untitled, so it takes its first prompt's
    /// title. Then asks for a generated title for whichever it titled, without waiting. A failure is only logged,
    /// because the prompt matters more than what it's called.
    pub(crate) async fn title_from_prompt(
        self: &Arc<Self>,
        session: &SessionId,
        workspace: &WorkspaceId,
        prompt: &str,
    ) {
        let Some(title) = Title::from_first_line(prompt) else { return };
        let (session, workspace_id) = (session.clone(), workspace.clone());
        let titled = self
            .blocking(move |store| {
                let chat = store.retitle_session(&session, &title, Retitle::FirstLine)?;
                Ok((chat, store.retitle_workspace(&workspace_id, &title, Retitle::FirstLine)?))
            })
            .await;
        let (chat, titled_workspace) = match titled {
            Ok(titled) => titled,
            Err(e) => {
                warn!("could not title a chat or workspace from its prompt: {e}");
                return;
            }
        };
        let titled = Titled {
            agent: chat.record.agent,
            session: chat.changed.then(|| chat.record.id.clone()),
            workspace: titled_workspace.changed.then(|| workspace.clone()),
        };
        if titled_workspace.changed {
            self.notify(HubNotice::Workspace(titled_workspace.record));
        }
        if chat.changed {
            self.notify(HubNotice::Session(chat.record));
        }
        if titled.session.is_some() || titled.workspace.is_some() {
            let asked = prompt.chars().take(MOST_PROMPT_CHARS).collect();
            tokio::spawn(self.clone().generate_title(workspace.clone(), titled, asked));
        }
    }

    /// Puts a generated title wherever the prompt's first line still is, so one typed meanwhile stays. A failure is
    /// only logged, and leaves the first line.
    async fn generate_title(self: Arc<Self>, workspace: WorkspaceId, titled: Titled, prompt: String) {
        let title = match self.title_for(&workspace, titled.agent, prompt).await {
            Ok(title) => title,
            Err(e) => {
                warn!(%workspace, "could not generate a title: {e}");
                return;
            }
        };
        let replaced = self
            .blocking(move |store| {
                // The workspace first, so once the chat has its title, the workspace's is settled too.
                let workspace = match &titled.workspace {
                    Some(id) => Some(store.retitle_workspace(id, &title, Retitle::Generated)?),
                    None => None,
                };
                let chat = match &titled.session {
                    Some(id) => Some(store.retitle_session(id, &title, Retitle::Generated)?),
                    None => None,
                };
                Ok((workspace, chat))
            })
            .await;
        match replaced {
            Ok((workspace, chat)) => {
                if let Some(workspace) = workspace.filter(|w| w.changed) {
                    self.notify(HubNotice::Workspace(workspace.record));
                }
                if let Some(chat) = chat.filter(|c| c.changed) {
                    self.notify(HubNotice::Session(chat.record));
                }
            }
            Err(e) => warn!(%workspace, "could not save a generated title: {e}"),
        }
    }

    /// Asks the workspace's worker, and keeps the answer only as a [`Title`], since a model can answer anything.
    async fn title_for(
        self: &Arc<Self>,
        workspace: &WorkspaceId,
        agent: AgentKind,
        prompt: String,
    ) -> Result<Title, ApiError> {
        let link = self.ensure_worker(workspace).await?;
        let reply = link.call(WorkerRequest::TitleFor { agent, prompt }).await?;
        let WorkerResponse::Title { title } = reply else { return Err(unexpected(&reply)) };
        Title::from_first_line(&title).ok_or_else(|| ApiError::new(ErrorCode::Worker, "the worker gave an empty title"))
    }
}

/// What a prompt titled from its first line, so its generated title goes to the same places, and whose agent to ask.
struct Titled {
    agent: AgentKind,
    session: Option<SessionId>,
    workspace: Option<WorkspaceId>,
}
