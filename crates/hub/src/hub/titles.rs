//! What workspaces and chats are called: titled from the first prompt, then by a generated title, or renamed by hand,
//! and a workspace titled again when someone asks. A title is generated from what was typed and the images that came
//! with it, since a prompt such as "how do we fix this?" can be about a screenshot.

use std::fmt::Write as _;
use std::sync::Arc;

use protocol::attachment::{Attachment, AttachmentBytes, NewAttachment};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AgentKind, PromptLine, Session, SessionId, Title, Workspace, WorkspaceId};
use protocol::prompt::Prompt;
use protocol::short_text::start_of;
use protocol::worker::TitleFor;
use tracing::warn;

use super::Hub;
use super::ready::ReadyWorkspace;
use crate::attachment_files::FileFor;
use crate::store::Retitle;

/// Enough of a prompt to say what it's about. Past this it's usually a pasted log, which only costs more to send and
/// to ask about.
const MOST_PROMPT_CHARS: usize = 4000;

/// The most images a title is asked from, and the most bytes of them. A few say what the work is about as well as
/// more would. The CLI shrinks a big one first: a full screenshot, 18 MB, took Haiku about three seconds and
/// 4,000 tokens, so this keeps a call well inside its timeout.
const MOST_IMAGES: usize = 3;
const MOST_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

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

    /// Titles the workspace again from what its chats have been asked and the images they were sent, and waits for the
    /// title, so whoever asked hears why there's none. The newest chat's agent is the one asked. Only for a workspace
    /// that's ready, as every read from a worker is, so asking never starts a worker for one that's archived or still
    /// being made.
    pub(crate) async fn workspace_regenerate_title(self: &Arc<Self>, id: WorkspaceId) -> Result<Workspace, ApiError> {
        let read = id.clone();
        let (workspace, agent, prompts, carried) = self
            .blocking(move |store| {
                let workspace = store.get_workspace(&read)?;
                let agent = store.list_sessions(&read)?.last().map(|chat| chat.agent);
                Ok((workspace, agent, store.workspace_prompt_lines(&read)?, store.workspace_prompt_images(&read)?))
            })
            .await?;
        let workspace = ReadyWorkspace::new(workspace)?.into_workspace();
        let images = self.read_images(chosen_images(carried)).await;
        // Prompts of only images have no words to list, so the model is told only what to make of the images.
        let asked = work_so_far(&prompts).or_else(|| (!images.is_empty()).then(|| WORK_SO_FAR.to_string()));
        let (Some(agent), Some(asked)) = (agent, asked) else {
            let message =
                format!("nothing has been asked in {} yet, so there's nothing to title it from", workspace.name);
            return Err(ApiError::new(ErrorCode::Conflict, message));
        };
        let title = self.title_for(&id, agent, asked, images).await?;
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
    /// same terms, so a workspace takes its first prompt's title. Then asks for a generated title for whichever it
    /// titled, without waiting. A prompt of only images has no first line, so a chat or workspace with no title stays
    /// untitled until the generated title comes, or if none does, until a prompt with words. A failure is only logged,
    /// because the prompt matters more than what it's called.
    pub(crate) async fn title_from_prompt(
        self: &Arc<Self>,
        session: &SessionId,
        workspace: &WorkspaceId,
        prompt: &Prompt<Attachment>,
    ) {
        let images = chosen_images(prompt.files.iter().map(|attachment| (session.clone(), attachment.clone())));
        let titled = match Title::from_first_line(&prompt.text) {
            Some(title) => self.title_from_first_line(session, workspace, title).await,
            None if !images.is_empty() => self.untitled(session, workspace).await,
            None => return,
        };
        let titled = match titled {
            Ok(titled) => titled,
            Err(e) => {
                warn!("could not title a chat or workspace from its prompt: {e}");
                return;
            }
        };
        if titled.session.is_some() || titled.workspace.is_some() {
            let asked = start_of(&prompt.text, MOST_PROMPT_CHARS).0.to_owned();
            tokio::spawn(self.clone().generate_title(workspace.clone(), titled, asked, images));
        }
    }

    /// Gives the chat and its workspace the prompt's first line wherever they have no title, and says so.
    async fn title_from_first_line(
        &self,
        session: &SessionId,
        workspace: &WorkspaceId,
        title: Title,
    ) -> Result<Titled, ApiError> {
        let (session, workspace_id) = (session.clone(), workspace.clone());
        let (chat, titled_workspace) = self
            .blocking(move |store| {
                let chat = store.retitle_session(&session, &title, Retitle::FirstLine)?;
                Ok((chat, store.retitle_workspace(&workspace_id, &title, Retitle::FirstLine)?))
            })
            .await?;
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
        Ok(titled)
    }

    /// Which of the chat and its workspace have no title, for a prompt with no first line to give them.
    async fn untitled(&self, session: &SessionId, workspace: &WorkspaceId) -> Result<Titled, ApiError> {
        let (session, workspace) = (session.clone(), workspace.clone());
        self.blocking(move |store| {
            let chat = store.get_session(&session)?.session;
            let untitled_workspace = store.get_workspace(&workspace)?.title.is_none();
            Ok(Titled {
                agent: chat.agent,
                session: chat.title.is_none().then_some(chat.id),
                workspace: untitled_workspace.then_some(workspace),
            })
        })
        .await
    }

    /// Puts a generated title wherever the prompt's first line still is, or where there's still none, so one typed
    /// meanwhile stays. A failure is only logged, and leaves the first line.
    async fn generate_title(
        self: Arc<Self>,
        workspace: WorkspaceId,
        titled: Titled,
        prompt: String,
        images: Vec<(SessionId, Attachment)>,
    ) {
        let images = self.read_images(images).await;
        if images.is_empty() && Title::from_first_line(&prompt).is_none() {
            warn!(%workspace, "could not generate a title: the hub no longer has the images of a prompt with no words");
            return;
        }
        let title = match self.title_for(&workspace, titled.agent, prompt, images).await {
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

    /// Asks the workspace's worker, under the login a launch there would get, and keeps the answer only as a [`Title`],
    /// since a model can answer anything.
    async fn title_for(
        self: &Arc<Self>,
        workspace: &WorkspaceId,
        agent: AgentKind,
        prompt: String,
        images: Vec<NewAttachment>,
    ) -> Result<Title, ApiError> {
        let link = self.ensure_worker(workspace).await?;
        let login = self.login_for(&self.workspace(workspace).await?, agent).await?.map(|handed| handed.login);
        let title = link.call(TitleFor { agent, prompt, images, login }).await?.title;
        Title::from_first_line(&title).ok_or_else(|| ApiError::new(ErrorCode::Worker, "the worker gave an empty title"))
    }

    /// The bytes of each image, as the hub keeps them. One it no longer has, or can't read, is left out, since a
    /// title from the words alone is better than none.
    async fn read_images(&self, chosen: Vec<(SessionId, Attachment)>) -> Vec<NewAttachment> {
        let mut images = Vec::with_capacity(chosen.len());
        for (session, attachment) in chosen {
            match self.attachment_files.read(&FileFor::Session(session.clone()), &attachment.id).await {
                Ok(Some(bytes)) => images.push(NewAttachment { attachment, data: AttachmentBytes::new(bytes) }),
                Ok(None) => {}
                Err(e) => warn!(%session, attachment = %attachment.id, "could not read an image to title from: {e}"),
            }
        }
        images
    }
}

/// Which of the images that prompts carried, oldest first, a title is asked from: the first ones, as many as
/// [`MOST_IMAGES`] and [`MOST_IMAGE_BYTES`] allow, passing over one too big for the bytes left. So the first prompt's,
/// which set the task, go first.
fn chosen_images(carried: impl IntoIterator<Item = (SessionId, Attachment)>) -> Vec<(SessionId, Attachment)> {
    let mut room = MOST_IMAGE_BYTES;
    let fits = |(_, attachment): &(SessionId, Attachment)| {
        let fits = attachment.media_type.is_image() && attachment.size_bytes <= room;
        if fits {
            room -= attachment.size_bytes;
        }
        fits
    };
    carried.into_iter().filter(fits).take(MOST_IMAGES).collect()
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
