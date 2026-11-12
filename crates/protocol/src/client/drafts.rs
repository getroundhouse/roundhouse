//! The requests that keep what's typed and not sent, so it's there after the app restarts, and in every client: in a
//! session's composer, in answer to its question, on a workspace's diff, and in the new-workspace dialog, with its
//! files. Each is the body of the `ClientRequest` of its name, which has the same form on the wire as if its fields
//! were the request's own. Keeping what's kept already changes nothing, so sending one again does nothing more. A hub
//! older than one refuses it, as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentBytes, MediaType};
use crate::model::{
    AnswerDraft, AttachmentId, CommentDraft, DraftToKeep, NewWorkspaceDraftToKeep, SessionId, WorkspaceId,
};

/// Keeps `draft` as what's typed and attached in the session's composer and not sent, in place of what was kept. An
/// empty one clears it. Answers with the session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionDraft {
    pub id: SessionId,
    pub draft: DraftToKeep,
}

/// Keeps `draft` as what's chosen and typed in answer to the question the session's agent is waiting on, in place of
/// what was kept. `None` clears it, as one with nothing chosen or typed does, and so does the question being answered
/// or cancelled, whoever answers it, after which one for it is left out. Answers with the session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionAnswerDraft {
    pub id: SessionId,
    pub draft: Option<AnswerDraft>,
}

/// Keeps `draft` as the comment being written on the workspace's diff, in place of what was kept. `None` clears it,
/// which a client sends once the comment is cancelled. Saving it lets go of it on the hub, as `CommentAdd` and
/// `CommentEdit` say, so a client has the hub keep what it saves before saving it, rather than clearing it first, which
/// would lose it if the save failed. One on a side of a diff the hub doesn't know is refused, as `CommentAdd` refuses
/// it. Answers with the workspace.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceCommentDraft {
    pub id: WorkspaceId,
    pub draft: Option<CommentDraft>,
}

/// Keeps `draft` as what's in the new-workspace dialog, as [`NewWorkspaceDraft`](crate::model::NewWorkspaceDraft) says, in
/// place of what was kept. Answers with the draft as kept.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftKeep {
    pub draft: NewWorkspaceDraftToKeep,
}

/// A file for the new-workspace dialog's draft to name, kept by the hub before any workspace or chat exists to add it
/// to, with the same checks as `AttachmentAdd`. Sent again with the same ID, it returns what the first one added
/// without looking at the rest.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftFileAdd {
    pub id: AttachmentId,
    pub name: String,
    pub media_type: MediaType,
    pub data: AttachmentBytes,
}

/// A file's bytes, for putting the draft's files back in the dialog, such as in another client. Not found once the hub
/// has let go of it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftFileRead {
    pub id: AttachmentId,
}

/// Lets go of a file the draft no longer names, and of its bytes. The hub keeps one the draft still names. One that
/// isn't there changes nothing. Answers with `Ok` either way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewWorkspaceDraftFileRemove {
    pub id: AttachmentId,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::client::ClientRequest;
    use crate::model::{AnswerSoFar, DiffSide};

    fn value(of: &impl Serialize) -> Value {
        serde_json::to_value(of).unwrap()
    }

    /// The same as when their fields were the requests' own, so a peer from before reads them.
    #[test]
    fn a_request_s_body_travels_beside_its_method() {
        let draft = DraftToKeep::new("half");
        let answer = AnswerDraft { approval_id: "q".into(), answers: vec![AnswerSoFar::default()] };
        let comment =
            CommentDraft { path: "a.txt".into(), side: DiffSide::New, line: 3, editing: None, text: "why?".into() };
        let dialog =
            NewWorkspaceDraftToKeep { text: "fix the login".into(), files: vec!["f1".into()], ..Default::default() };
        let data = AttachmentBytes::new(b"a line".to_vec());
        let requests = [
            (
                ClientRequest::SessionDraft(SessionDraft { id: "s1".into(), draft: draft.clone() }),
                json!({"method": "session_draft", "id": "s1", "draft": value(&draft)}),
            ),
            (
                ClientRequest::SessionAnswerDraft(SessionAnswerDraft { id: "s1".into(), draft: Some(answer.clone()) }),
                json!({"method": "session_answer_draft", "id": "s1", "draft": value(&answer)}),
            ),
            (
                ClientRequest::SessionAnswerDraft(SessionAnswerDraft { id: "s1".into(), draft: None }),
                json!({"method": "session_answer_draft", "id": "s1", "draft": null}),
            ),
            (
                ClientRequest::WorkspaceCommentDraft(WorkspaceCommentDraft {
                    id: "w1".into(),
                    draft: Some(comment.clone()),
                }),
                json!({"method": "workspace_comment_draft", "id": "w1", "draft": value(&comment)}),
            ),
            (
                ClientRequest::NewWorkspaceDraftKeep(NewWorkspaceDraftKeep { draft: dialog.clone() }),
                json!({"method": "new_workspace_draft_keep", "draft": value(&dialog)}),
            ),
            (
                ClientRequest::NewWorkspaceDraftFileAdd(NewWorkspaceDraftFileAdd {
                    id: "f1".into(),
                    name: "notes.txt".into(),
                    media_type: MediaType::Text,
                    data: data.clone(),
                }),
                json!({"method": "new_workspace_draft_file_add", "id": "f1", "name": "notes.txt", "media_type": value(&MediaType::Text), "data": value(&data)}),
            ),
            (
                ClientRequest::NewWorkspaceDraftFileRead(NewWorkspaceDraftFileRead { id: "f1".into() }),
                json!({"method": "new_workspace_draft_file_read", "id": "f1"}),
            ),
            (
                ClientRequest::NewWorkspaceDraftFileRemove(NewWorkspaceDraftFileRemove { id: "f1".into() }),
                json!({"method": "new_workspace_draft_file_remove", "id": "f1"}),
            ),
        ];

        for (request, form) in requests {
            let sent = serde_json::to_value(&request).unwrap();

            assert_eq!(sent, form);
            assert_eq!(serde_json::from_value::<ClientRequest>(sent).unwrap(), request);
        }
    }
}
