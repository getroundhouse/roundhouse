//! Making a workspace from what the new-workspace dialog held, and sending its first chat the prompt. The hub does it
//! from what it keeps of the dialog, files and all. A hub too old to is sent the workspace a step at a time, with the
//! files read back from it. It runs on the thread `Client::start` gives it, since each call blocks.

use client_core::{Client, ClientError, FileToSend, Written};
use protocol::attachment::Attachment;
use protocol::client::ApiError;
use protocol::client::new_workspace::{NewWorkspaceMade, UnsentPrompt};
use protocol::model::{NewWorkspaceDraftToKeep, PermissionMode, Session, SessionSettingsChange, Workspace};
use protocol::prompt::Prompt;

use crate::new_workspace::Request;

#[cfg(test)]
mod tests;

/// How far making a workspace got.
pub(super) struct Made {
    pub(super) workspace: Option<Workspace>,
    /// Its first chat.
    pub(super) session: Option<Session>,
    /// The prompt, if a step failed before it was sent.
    pub(super) unsent: Option<Unsent>,
    /// What a workspace made a step at a time took from the dialog, for the dialog to stop keeping on the hub, which
    /// doesn't when it makes the workspace itself.
    pub(super) dialog_to_empty: Option<NewWorkspaceDraftToKeep>,
}

pub(super) struct Unsent {
    /// What wasn't sent, to give back, or `None` once the hub has put it in the new chat's composer itself.
    pub(super) given_back: Option<GivenBack>,
    /// The mode the chat was to start in, which goes back with the prompt.
    pub(super) permission_mode: PermissionMode,
    pub(super) failed: Step,
    pub(super) error: ClientError,
}

/// A prompt that wasn't sent, for the dialog or the new chat's composer to put back.
pub(super) struct GivenBack {
    pub(super) prompt: Prompt<FileToSend>,
    /// Files the hub keeps for the dialog that weren't read back, which only the dialog can put back.
    pub(super) coming_back: Vec<Attachment>,
    /// Whether the hub still keeps it as what's in the dialog, as one it couldn't give the new chat whole, so it goes
    /// back there, as one is when nothing was made, rather than to the new chat's composer.
    pub(super) in_the_dialog: bool,
}

impl Unsent {
    /// All of `request`, given back, since `failed` failed before anything was sent.
    fn of(request: Request, failed: Step, error: ClientError) -> Self {
        let Request { prompt, coming_back, permission_mode, .. } = request;
        let given_back = Some(GivenBack { prompt, coming_back, in_the_dialog: false });
        Self { given_back, permission_mode, failed, error }
    }
}

/// A step in making a workspace, each of which can fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Workspace,
    Chat,
    /// Giving the chat the model, effort and mode chosen. The prompt isn't sent without them, so a chat that's to plan
    /// doesn't start on it with full access.
    Model,
    Prompt,
    /// Hearing back from the hub, which may have made it.
    Answer,
}

impl Step {
    /// What to say when this step failed.
    pub(super) fn failed(self, e: &ClientError) -> String {
        match self {
            Step::Workspace => format!("couldn't create the workspace: {e}"),
            Step::Chat => format!("couldn't start the workspace's chat: {e}"),
            Step::Model => format!("couldn't give the chat the model, effort and mode chosen: {e}"),
            Step::Prompt => format!("couldn't send that: {e}"),
            Step::Answer => format!("couldn't tell whether the workspace was made: {e}"),
        }
    }
}

/// Has the hub make it from what it keeps of the dialog, once that's what's asked for. A refusal comes before anything
/// is made, so all of it is given back.
pub(super) fn make(client: &Client, request: Request) -> Made {
    if let Some(kept) = &request.kept {
        // An answer either way, so a hub that's gone can't hold this forever.
        let _ = kept.recv();
    }
    match client.new_workspace_create(&request.made_of, client_core::new_request_id()) {
        Ok(NewWorkspaceMade { workspace, session, unchosen, unsent }) => {
            let unsent = unsent_by_the_hub(request, unchosen, unsent);
            Made { workspace: Some(workspace), session: Some(session), unsent, dialog_to_empty: None }
        }
        Err(e) if e.is_unsupported() => step_by_step(client, request),
        Err(error) => {
            // A refusal comes before anything is made. Anything else, such as the connection going, may have come
            // after.
            let failed = if matches!(error, ClientError::Api(_)) { Step::Workspace } else { Step::Answer };
            let unsent = Some(Unsent::of(request, failed, error));
            Made { workspace: None, session: None, unsent, dialog_to_empty: None }
        }
    }
}

/// What the hub says it didn't do with the prompt of a workspace it made. One in the new chat's composer is there
/// already. One the hub couldn't give the chat whole is still in the dialog, which is where it goes back, with the
/// files the dialog hadn't read back. A chat that couldn't be given what was chosen is said to have failed at that.
fn unsent_by_the_hub(request: Request, unchosen: Option<ApiError>, unsent: Option<UnsentPrompt>) -> Option<Unsent> {
    let failed = if unchosen.is_some() { Step::Model } else { Step::Prompt };
    let permission_mode = request.permission_mode;
    match (unsent, unchosen) {
        (Some(UnsentPrompt { error, in_composer: false }), unchosen) => {
            let mut unsent = Unsent::of(request, failed, ClientError::Api(unchosen.unwrap_or(error)));
            if let Some(given_back) = &mut unsent.given_back {
                given_back.in_the_dialog = true;
            }
            Some(unsent)
        }
        (Some(UnsentPrompt { error, in_composer: true }), unchosen) => {
            let error = ClientError::Api(unchosen.unwrap_or(error));
            Some(Unsent { given_back: None, permission_mode, failed, error })
        }
        // Nothing was typed, so there's nothing to give back, but the chat still isn't as chosen.
        (None, Some(unchosen)) => {
            Some(Unsent { given_back: None, permission_mode, failed, error: ClientError::Api(unchosen) })
        }
        (None, None) => None,
    }
}

/// Makes it a step at a time, for a hub too old to make it from what it keeps, reading back first the files it keeps
/// for the dialog that the dialog hadn't, to send them.
fn step_by_step(client: &Client, mut request: Request) -> Made {
    let made_of = request.made_of.clone();
    let mut read = Vec::new();
    for file in &request.coming_back {
        match client.new_workspace_draft_file_read(&file.id) {
            Ok(data) => read.push(FileToSend { attachment: file.clone(), data }),
            Err(error) => {
                let unsent = Some(Unsent::of(request, Step::Workspace, error));
                return Made { workspace: None, session: None, unsent, dialog_to_empty: None };
            }
        }
    }
    request.coming_back.clear();
    read.append(&mut request.prompt.files);
    request.prompt.files = read;
    let mut made = Made { workspace: None, session: None, unsent: None, dialog_to_empty: None };
    // Under one ID however far it gets, so a retry can't record it twice.
    let written = Written::new(std::mem::take(&mut request.prompt));
    if let Err((failed, error)) = make_into(client, &request, &written, &mut made) {
        request.prompt = written.prompt;
        made.unsent = Some(Unsent::of(request, failed, error));
    }
    made.dialog_to_empty = made.workspace.is_some().then_some(made_of);
    made
}

/// Each step, recording in `made` what it made, until one fails.
fn make_into(
    client: &Client,
    request: &Request,
    written: &Written,
    made: &mut Made,
) -> Result<(), (Step, ClientError)> {
    let at = |step| move |error| (step, error);
    let workspace = client
        .workspace_create(&request.repo, None, None, Some(client_core::new_request_id()))
        .map_err(at(Step::Workspace))?;
    let id = workspace.id.clone();
    made.workspace = Some(workspace);
    // The workspace's first chat is found or made by the hub, so two clients opening it at once share one.
    let session = client.session_ensure(&id, request.agent).map_err(at(Step::Chat))?;
    let chat = session.id.clone();
    let chosen = as_chosen(&session, request);
    made.session = Some(session);
    if !chosen {
        let (model, effort) = (request.model.clone(), request.effort);
        let options = SessionSettingsChange { model, permission_mode: Some(request.permission_mode), effort };
        made.session = Some(client.session_configure(&chat, options).map_err(at(Step::Model))?);
    }
    if written.prompt.says_or_carries() {
        client.send_prompt(&chat, written).map_err(at(Step::Prompt))?;
    }
    Ok(())
}

/// Whether the chat has the model, effort and mode `request` chose. The hub gave the chat what its agent starts a chat
/// with, or another client made it, and either may be what was chosen. Nothing chosen is what the agent starts with.
fn as_chosen(session: &Session, request: &Request) -> bool {
    request.model.as_ref().is_none_or(|model| *model == session.model)
        && request.effort.is_none_or(|effort| effort == session.effort)
        && session.permission_mode == request.permission_mode
}
