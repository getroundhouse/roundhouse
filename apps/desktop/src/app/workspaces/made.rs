//! Making a workspace from what the new-workspace dialog held, then sending its first chat the prompt. It runs on the
//! thread `Client::start` gives it, since each call blocks, and each step needs the one before.

use client_core::{Client, ClientError, FileToSend, Written};
use protocol::model::{PermissionMode, Session, SessionSettingsChange, Workspace};
use protocol::prompt::Prompt;

use crate::new_workspace::Request;

/// How far making a workspace got.
pub(super) struct Made {
    pub(super) workspace: Option<Workspace>,
    /// Its first chat.
    pub(super) session: Option<Session>,
    /// The prompt, if a step failed before it was sent.
    pub(super) unsent: Option<Unsent>,
}

pub(super) struct Unsent {
    pub(super) prompt: Prompt<FileToSend>,
    /// The mode the chat was to start in, which goes back with the prompt.
    pub(super) permission_mode: PermissionMode,
    pub(super) failed: Step,
    pub(super) error: ClientError,
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
}

impl Step {
    /// What to say when this step failed.
    pub(super) fn failed(self, e: &ClientError) -> String {
        match self {
            Step::Workspace => format!("couldn't create the workspace: {e}"),
            Step::Chat => format!("couldn't start the workspace's chat: {e}"),
            Step::Model => format!("couldn't give the chat the model, effort and mode chosen: {e}"),
            Step::Prompt => format!("couldn't send that: {e}"),
        }
    }
}

pub(super) fn make(client: &Client, mut request: Request) -> Made {
    let mut made = Made { workspace: None, session: None, unsent: None };
    // Under one ID however far it gets, so a retry can't record it twice.
    let written = Written::new(std::mem::take(&mut request.prompt));
    if let Err((failed, error)) = make_into(client, &request, &written, &mut made) {
        made.unsent = Some(Unsent { prompt: written.prompt, permission_mode: request.permission_mode, failed, error });
    }
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
    // The hub gave the chat what its agent starts a chat with, or another client made it, and either may be what was
    // chosen. Nothing chosen is what the agent starts with.
    let as_chosen = request.model.as_ref().is_none_or(|model| *model == session.model)
        && request.effort.is_none_or(|effort| effort == session.effort)
        && session.permission_mode == request.permission_mode;
    made.session = Some(session);
    if !as_chosen {
        let (model, effort) = (request.model.clone(), request.effort);
        let options = SessionSettingsChange { model, permission_mode: Some(request.permission_mode), effort };
        made.session = Some(client.session_configure(&chat, options).map_err(at(Step::Model))?);
    }
    if written.prompt.says_or_carries() {
        client.send_prompt(&chat, written).map_err(at(Step::Prompt))?;
    }
    Ok(())
}
