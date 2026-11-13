//! Making a workspace from what's in the new-workspace dialog, as the hub keeps it, so a client sends what's made by
//! what the hub has, rather than reading back the dialog's files and sending them again.

use serde::{Deserialize, Serialize};

use super::{ApiError, ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ClientRequestId, NewWorkspaceDraftToKeep, Session, Workspace};

/// Makes a workspace from what's in the new-workspace dialog, as `made_of` says it is, as Create does: the workspace, in
/// its repository, and its first chat, with the model, effort and mode picked. The chat is sent the prompt, if there
/// is one, carrying copies of the files the hub keeps for the dialog. A file `made_of` names that the hub doesn't keep
/// for the dialog is refused before anything is made, and so are more files than a prompt can carry. The hub goes on
/// keeping what's in the dialog until the chat has the prompt, so nothing is lost if the client stops meanwhile. Then
/// it empties it, keeping the repository, model and effort, if it still says what `made_of` does, so what's typed in
/// the dialog since is kept. A prompt the chat can't be sent goes to its composer, if every file it carries can be
/// copied to the chat, and otherwise stays in the dialog. Sent again with the same `client_request_id`, it makes
/// nothing more. Answers with what was made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceCreate {
    pub made_of: NewWorkspaceDraftToKeep,
    pub client_request_id: ClientRequestId,
}

impl Request for NewWorkspaceCreate {
    type Reply = NewWorkspaceMade;

    fn into_request(self) -> ClientRequest {
        ClientRequest::NewWorkspaceCreate(self)
    }
}

/// What [`NewWorkspaceCreate`] made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceMade {
    pub workspace: Workspace,
    /// Its first chat.
    pub session: Session,
    /// Why the chat couldn't be given the model, effort and mode picked, if it couldn't. Its prompt isn't sent then, so
    /// a chat that's to plan doesn't start on it with full access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unchosen: Option<ApiError>,
    /// The prompt, if there was one and it wasn't sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsent: Option<UnsentPrompt>,
}

impl Reply for NewWorkspaceMade {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::NewWorkspaceMade(made) = response else { return Err(response) };
        Ok(*made)
    }
}

/// A prompt [`NewWorkspaceCreate`] didn't send, and where it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UnsentPrompt {
    /// Why it wasn't sent. For a chat that couldn't be given what was picked, what `unchosen` says.
    pub error: ApiError,
    /// Whether the chat's composer has it, with every file it carries. Otherwise it's only what's in the dialog, which
    /// the hub goes on keeping, files and all.
    pub in_composer: bool,
}
