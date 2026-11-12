//! The requests that keep what's typed and not sent, so it's there after the app restarts, and in every client: in a
//! session's composer, in answer to its question, and on a workspace's diff. Each is the body of the `ClientRequest`
//! of its name, which has the same form on the wire as if its fields were the request's own. Keeping what's kept
//! already changes nothing, so sending one again does nothing more. A hub older than one refuses it, as
//! `is_unsupported` says.

use serde::{Deserialize, Serialize};

use crate::model::{AnswerDraft, CommentDraft, NewDraft, SessionId, WorkspaceId};

/// Keeps `draft` as what's typed and attached in the session's composer and not sent, in place of what was kept. An
/// empty one clears it. Answers with the session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionDraft {
    pub id: SessionId,
    pub draft: NewDraft,
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
/// which a client sends once the comment is saved or cancelled, and one on a side of a diff the hub doesn't know is
/// refused, as `CommentAdd` refuses it. Answers with the workspace.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceCommentDraft {
    pub id: WorkspaceId,
    pub draft: Option<CommentDraft>,
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
        let draft = NewDraft::new("half");
        let answer = AnswerDraft { approval_id: "q".into(), answers: vec![AnswerSoFar::default()] };
        let comment =
            CommentDraft { path: "a.txt".into(), side: DiffSide::New, line: 3, editing: None, text: "why?".into() };
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
        ];

        for (request, form) in requests {
            let sent = serde_json::to_value(&request).unwrap();

            assert_eq!(sent, form);
            assert_eq!(serde_json::from_value::<ClientRequest>(sent).unwrap(), request);
        }
    }
}
