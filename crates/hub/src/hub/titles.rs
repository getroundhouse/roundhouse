//! What workspaces and chats are called: titled from the first prompt, or renamed by hand.

use protocol::client::{ApiError, HubNotice};
use protocol::model::{Session, SessionId, Title, Workspace, WorkspaceId};
use tracing::warn;

use super::Hub;
use crate::store::Retitle;

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
    /// title. A failure is only logged, because the prompt matters more than what it's called.
    pub(crate) async fn title_from_prompt(&self, session: &SessionId, workspace: &WorkspaceId, prompt: &str) {
        let Some(title) = Title::from_first_line(prompt) else { return };
        let (session, workspace) = (session.clone(), workspace.clone());
        let titled = self
            .blocking(move |store| {
                let chat = store.retitle_session(&session, &title, Retitle::FirstLine)?;
                let workspace = store.retitle_workspace(&workspace, &title, Retitle::FirstLine)?;
                Ok((chat.changed.then_some(chat.record), workspace.changed.then_some(workspace.record)))
            })
            .await;
        let (chat, workspace) = match titled {
            Ok(titled) => titled,
            Err(e) => {
                warn!("could not title a chat or workspace from its prompt: {e}");
                return;
            }
        };
        if let Some(workspace) = workspace {
            self.notify(HubNotice::Workspace(workspace));
        }
        if let Some(chat) = chat {
            self.notify(HubNotice::Session(chat));
        }
    }
}
