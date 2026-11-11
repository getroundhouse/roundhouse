//! What workspaces and chats are called: titled from the first prompt, then by a generated title, or renamed by hand,
//! and a workspace titled again when someone asks.

use std::fmt::Write as _;
use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AgentKind, PromptLine, Session, SessionId, Title, Workspace, WorkspaceId, WorkspaceStatus};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tracing::warn;

use super::{Hub, not_ready, unexpected};
use crate::store::Retitle;

/// Enough of a prompt to say what it's about. Past this it's usually a pasted log, which only costs more to send and
/// to ask about.
const MOST_PROMPT_CHARS: usize = 4000;

/// What a model is told before a workspace's prompts, when it's asked to title the work again. Without it, a small
/// model names the newest prompt, joins two things with "and", or names a chore such as merging, which most
/// workspaces end with.
const WORK_SO_FAR: &str = "The task is what all of these messages add up to. Someone sent them to an agent, oldest \
     first, while working on it. Name the one thing the work is about, as a whole, not its latest step, and without \
     joining two things with 'and'. Every task ends with committing, a pull request, a review, checks and merging, \
     so never mention those.";

#[cfg(test)]
mod tests;

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

    /// Titles the workspace again from what its chats have been asked, and waits for the title, so whoever asked hears
    /// why there's none. The newest chat's agent is the one asked. Only for a workspace that's ready, as every read
    /// from a worker is, so asking never starts a worker for one that's archived or still being made.
    pub(crate) async fn workspace_regenerate_title(self: &Arc<Self>, id: WorkspaceId) -> Result<Workspace, ApiError> {
        let read = id.clone();
        let (workspace, agent, prompts) = self
            .blocking(move |store| {
                let workspace = store.get_workspace(&read)?;
                let agent = store.list_sessions(&read)?.last().map(|chat| chat.agent);
                Ok((workspace, agent, store.workspace_prompt_lines(&read)?))
            })
            .await?;
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let (Some(agent), Some(asked)) = (agent, work_so_far(&prompts)) else {
            let message =
                format!("nothing has been asked in {} yet, so there's nothing to title it from", workspace.name);
            return Err(ApiError::new(ErrorCode::Conflict, message));
        };
        let title = self.title_for(&id, agent, asked).await?;
        let (over, set) = (workspace.title, workspace.title_source);
        let retitled = self
            .blocking(move |store| {
                store.retitle_workspace(&id, &title, Retitle::Regenerated { over: over.as_ref(), set })
            })
            .await?;
        if retitled.changed {
            self.notify(HubNotice::Workspace(retitled.record.clone()));
        }
        Ok(retitled.record)
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

/// What a model is asked to title a workspace's work from: what its chats were asked, oldest first and numbered, under
/// [`WORK_SO_FAR`]. `None` if nothing was typed in any of them. When they'd come to more than [`MOST_PROMPT_CHARS`],
/// it's the first, which set the task, and the newest that fit, which say where it went, with a line of its own for
/// the gap.
fn work_so_far(prompts: &[PromptLine]) -> Option<String> {
    let typed: Vec<&str> = prompts.iter().map(PromptLine::text).filter(|text| !text.is_empty()).collect();
    let (first, later) = typed.split_first()?;
    let mut room = MOST_PROMPT_CHARS.saturating_sub(first.chars().count());
    let fits = |text: &&&str| {
        let fits = text.chars().count() <= room;
        room = room.saturating_sub(text.chars().count());
        fits
    };
    let newest = later.iter().rev().take_while(fits).count();
    let left_out = later.len() - newest;
    let mut asked = format!("{WORK_SO_FAR}\n\n1. {first}");
    if left_out > 0 {
        asked.push_str("\n…");
    }
    for (place, text) in later.iter().enumerate().skip(left_out) {
        // Writing to a string can't fail.
        let _ = write!(asked, "\n{}. {text}", place + 2);
    }
    Some(asked)
}

/// What a prompt titled from its first line, so its generated title goes to the same places, and whose agent to ask.
struct Titled {
    agent: AgentKind,
    session: Option<SessionId>,
    workspace: Option<WorkspaceId>,
}
