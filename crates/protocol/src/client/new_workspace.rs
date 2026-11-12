//! Making a workspace from what's in the new-workspace dialog, as the hub keeps it, so a client sends what's made by
//! what the hub has, rather than reading back the dialog's files and sending them again.

use serde::{Deserialize, Serialize};

use super::ApiError;
use crate::model::{ClientRequestId, NewWorkspaceDraftToKeep, Session, Workspace};

/// Makes a workspace from what's in the new-workspace dialog, as `made_of` says it is, as Create does: the workspace, in
/// its repository, and its first chat, with the model, effort and mode picked. The chat is sent the prompt, if there
/// is one, carrying copies of the files the hub keeps for the dialog. A file `made_of` names that the hub doesn't keep
/// for the dialog is refused before anything is made, and so are more files than a prompt can carry. The hub goes on
/// keeping what's in the dialog until the chat has the prompt, so nothing is lost if the client stops meanwhile. Then
/// it empties it, keeping the repository, model and effort, if it still says what `made_of` does, so what's typed in
/// the dialog since is kept. A prompt the chat can't be sent goes to its composer. Sent again with the same
/// `client_request_id`, it makes nothing more. Answers with what was made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceCreate {
    pub made_of: NewWorkspaceDraftToKeep,
    pub client_request_id: ClientRequestId,
}

/// What [`NewWorkspaceCreate`] made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceMade {
    pub workspace: Workspace,
    /// Its first chat.
    pub session: Session,
    /// Why the prompt couldn't be sent, if it couldn't, in which case it's in the chat's composer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsent: Option<ApiError>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::client::ClientRequest;

    #[test]
    fn the_request_travels_as_its_fields_beside_its_method() {
        let made_of =
            NewWorkspaceDraftToKeep { text: "fix the login".into(), files: vec!["f1".into()], ..Default::default() };
        let request = ClientRequest::NewWorkspaceCreate(NewWorkspaceCreate {
            made_of: made_of.clone(),
            client_request_id: "make it".into(),
        });

        let sent = serde_json::to_value(&request).unwrap();

        let form = json!({
            "method": "new_workspace_create",
            "made_of": serde_json::to_value(&made_of).unwrap(),
            "client_request_id": "make it",
        });
        assert_eq!(sent, form);
        assert_eq!(serde_json::from_value::<ClientRequest>(sent).unwrap(), request);
    }
}
