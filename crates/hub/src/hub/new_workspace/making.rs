use super::super::Hub;
use crate::attachment_files::FileFor;
use crate::store::AddedBy;
use protocol::attachment::{Attachment, AttachmentBytes};
use protocol::client::new_workspace::{NewWorkspaceMade, UnsentPrompt};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{
    AttachmentId, ClientMessageId, ClientRequestId, DraftToKeep, NewWorkspaceDraftToKeep, PermissionMode, Session,
    SessionSettingsChange, Workspace, WorkspaceId,
};
use protocol::prompt::{Prompt, PromptProblem, says_or_carries};
use std::sync::Arc;
use tracing::warn;

impl Hub {
    /// Makes the workspace `made_of` says, as `NewWorkspaceCreate` asks, and empties the dialog once its first chat has
    /// the prompt, in its log or, if it couldn't be sent, in its composer.
    pub(crate) async fn new_workspace_create(
        self: &Arc<Self>,
        made_of: NewWorkspaceDraftToKeep,
        client_request_id: ClientRequestId,
    ) -> Result<NewWorkspaceMade, ApiError> {
        let Some(repo_id) = made_of.repo_id.clone() else {
            let message = "a workspace is made in a repository, and none was named";
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        };
        if let Err(problem @ PromptProblem::TooManyFiles) = first_prompt_of(&made_of).check() {
            return Err(problem.into());
        }
        // Sent again, the first may have taken the dialog's files to the chat, and let go of them.
        let again = self.made_earlier::<Workspace>(Some(&client_request_id)).await?.is_some();
        let files = self.dialog_files(&made_of, again).await?;
        let workspace = self.workspace_create(repo_id, None, None, Some(client_request_id.clone())).await?;
        let (session, unchosen) = self.first_chat(&workspace.id, &made_of).await?;
        let first = self.first_prompt(session, unchosen.clone(), &made_of, &files, &client_request_id);
        let (session, unsent, taken) = first.await;
        if taken {
            self.done_with_dialog(&made_of).await;
        }
        Ok(NewWorkspaceMade { workspace, session, unchosen, unsent })
    }

    /// Each file `made_of` names that the hub keeps for the dialog. One it doesn't is refused, unless the request was
    /// sent `again`, when the chat may have it already. Each waits as long again for the dialog to name it, so one
    /// the dialog has had for long isn't let go of while the workspace is made, if the dialog changes meanwhile.
    pub(super) async fn dialog_files(
        &self,
        made_of: &NewWorkspaceDraftToKeep,
        again: bool,
    ) -> Result<Vec<Attachment>, ApiError> {
        let ids = made_of.files.clone();
        let files = self.blocking(move |store| {
            store.renew_new_workspace_draft_files(&ids)?;
            ids.iter().map(|id| store.new_workspace_draft_file(id)).collect()
        });
        let files: Vec<Option<Attachment>> = files.await?;
        let mut found = Vec::new();
        for (file, id) in files.into_iter().zip(&made_of.files) {
            match file {
                Some(file) => found.push(file),
                None if again => {}
                None => {
                    let message = format!("the dialog has no file {id} on the hub");
                    return Err(ApiError::new(ErrorCode::NotFound, message));
                }
            }
        }
        Ok(found)
    }

    /// The workspace's first chat, found or made as a client opening it finds or makes one, so two at once share it,
    /// and given the model, effort and mode picked, unless it has them. Returns the chat, and why it couldn't be given
    /// them, if it couldn't.
    pub(super) async fn first_chat(
        self: &Arc<Self>,
        workspace: &WorkspaceId,
        made_of: &NewWorkspaceDraftToKeep,
    ) -> Result<(Session, Option<ApiError>), ApiError> {
        let session =
            self.session_ensure(workspace.clone(), made_of.agent.unwrap_or_else(|| self.new_chat_kind())).await?;
        let mode = if made_of.plan { PermissionMode::Plan } else { PermissionMode::FullAccess };
        let as_chosen = made_of.agent.is_none_or(|agent| agent == session.agent)
            && made_of.model.as_ref().is_none_or(|model| *model == session.model)
            && made_of.effort.is_none_or(|effort| effort == session.effort)
            && session.permission_mode == mode;
        if as_chosen {
            return Ok((session, None));
        }
        if made_of.agent.is_some_and(|agent| agent != session.agent) {
            return Ok((
                session,
                Some(ApiError::new(
                    protocol::client::ErrorCode::Conflict,
                    "the workspace already has a chat with another agent",
                )),
            ));
        }
        let (model, effort) = (made_of.model.clone(), made_of.effort);
        let chosen = SessionSettingsChange { model, permission_mode: Some(mode), effort };
        match self.session_configure(session.id.clone(), chosen).await {
            Ok(configured) => Ok((configured, None)),
            Err(unchosen) => Ok((session, Some(unchosen))),
        }
    }

    /// Copies the dialog's files to the chat and sends it the prompt, if there is one, or puts the prompt in the chat's
    /// composer if it can't be sent, saying why. It isn't sent to a chat that couldn't be given the model, effort and
    /// mode picked, which `unchosen` says, so one that's to plan doesn't start on it with full access. It goes to the
    /// chat only whole, so if a file can't be copied, or the composer can't be given it, it stays only in the dialog,
    /// which has every file. Returns the chat, the prompt if it wasn't sent, and whether the chat has it now, one way or
    /// the other, so the dialog can let it go.
    pub(super) async fn first_prompt(
        self: &Arc<Self>,
        session: Session,
        unchosen: Option<ApiError>,
        made_of: &NewWorkspaceDraftToKeep,
        files: &[Attachment],
        client_request_id: &ClientRequestId,
    ) -> (Session, Option<UnsentPrompt>, bool) {
        if !says_or_carries(&made_of.text, made_of.files.len()) {
            return (session, None, true);
        }
        if let Err(error) = self.copy_files(&session, files).await {
            return (session, Some(UnsentPrompt { error, in_composer: false }), false);
        }
        let sent = match unchosen {
            None => {
                let message = ClientMessageId::from(client_request_id.as_str());
                self.session_prompt(session.id.clone(), Some(message), first_prompt_of(made_of)).await.map(drop)
            }
            Some(unchosen) => Err(unchosen),
        };
        let Err(error) = sent else { return (session, None, true) };
        let draft = DraftToKeep { text: made_of.text.clone(), files: made_of.files.clone() };
        match self.session_draft(session.id.clone(), draft).await {
            Ok(kept) => (kept, Some(UnsentPrompt { error, in_composer: true }), true),
            Err(e) => {
                warn!(session = %session.id, "couldn't keep a new workspace's first prompt in its composer: {e}");
                (session, Some(UnsentPrompt { error, in_composer: false }), false)
            }
        }
    }

    /// Copies each of the dialog's files to the chat, as a client adds one, unless the chat has it already.
    pub(super) async fn copy_files(&self, session: &Session, files: &[Attachment]) -> Result<(), ApiError> {
        for file in files {
            let (session_id, id) = (session.id.clone(), file.id.clone());
            if self.blocking(move |store| store.attachment(&session_id, &id)).await?.is_some() {
                continue;
            }
            let Some(data) = self.attachment_files.read(&FileFor::NewWorkspace, &file.id).await? else {
                return Err(ApiError::new(ErrorCode::NotFound, format!("the hub no longer has {}", file.name)));
            };
            let (id, name, data) = (file.id.clone(), file.name.clone(), AttachmentBytes::new(data));
            let kept_for = FileFor::Session(session.id.clone());
            self.file_add(kept_for, id, name, file.media_type, data, AddedBy::Client).await?;
        }
        Ok(())
    }

    /// Empties what's in the dialog, keeping where it makes a workspace and how its chat runs, as Create leaves it, if
    /// it still says what was made, and lets go of the dialog's files that nothing names now. What's typed in it since,
    /// such as in another client, stays, with the files it names.
    pub(super) async fn done_with_dialog(&self, made_of: &NewWorkspaceDraftToKeep) {
        let now = made_of.clone();
        let emptied = NewWorkspaceDraftToKeep {
            repo_id: made_of.repo_id.clone(),
            agent: made_of.agent,
            model: made_of.model.clone(),
            effort: made_of.effort,
            ..NewWorkspaceDraftToKeep::default()
        };
        match self.blocking(move |store| store.replace_new_workspace_draft(&now, &emptied)).await {
            Ok((kept, true)) => self.notify(HubNotice::NewWorkspaceDraft(kept)),
            Ok((_, false)) => {}
            Err(e) => warn!("couldn't empty the new-workspace dialog: {e}"),
        }
        for id in &made_of.files {
            if let Err(e) = self.file_remove(FileFor::NewWorkspace, id.clone()).await {
                warn!(attachment = %id, "couldn't let go of a file of the new-workspace dialog's: {e}");
            }
        }
    }
}

/// The prompt the dialog sends its workspace's first chat: what's typed, carrying the files it names.
fn first_prompt_of(made_of: &NewWorkspaceDraftToKeep) -> Prompt<AttachmentId> {
    Prompt::carrying(made_of.text.clone(), made_of.files.clone())
}
