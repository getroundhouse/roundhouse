//! What's typed and not sent: in a chat's composer, with what's attached there, in answer to a chat's question, in a
//! comment on a workspace's diff, and in the new-workspace dialog. The hub keeps each, so it's there after the app
//! restarts, and in every client.

use serde::{Deserialize, Serialize};

use crate::attachment::Attachment;
use crate::model::{ApprovalId, AttachmentId, CommentId, DiffSide, Effort, ModelName, RepoId, Rev};

/// What's in the new-workspace dialog and not made yet: where the workspace is to be made, what's typed and attached
/// for its first chat, and how that chat is to run. Kept on the hub, so it's there after the app restarts, and in
/// every client, such as on a phone after it was started on a Mac. A hub keeps one, since a person writes one at a
/// time, and like a chat's draft it's the person's, which a hub with several people would keep for each.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct NewWorkspaceDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<crate::model::AgentKind>,
    /// The repository it's to be made in, which may have gone since. `None` until something's kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<RepoId>,
    /// What's typed, which can be empty when files are attached.
    #[serde(default)]
    pub text: String,
    /// The files waiting to go with it, in the order they were attached, as the hub has them. No workspace or chat
    /// exists yet to add them to, so the hub keeps them for the draft until it lets go of each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Attachment>,
    /// The model and effort picked for its first chat, or `None` for what the hub gives a chat that names neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Its first chat is to start in plan mode, rather than with full access.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
    #[serde(default)]
    pub rev: Rev,
}

/// What a client asks the hub to keep as its [`NewWorkspaceDraft`], in place of what's there: the same, with the files by
/// ID. Each file is added first, as a chat's draft's are, so the hub names each as it has it, and leaves out any it
/// hasn't been given.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct NewWorkspaceDraftToKeep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<crate::model::AgentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<RepoId>,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<AttachmentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
}

/// What's typed and attached in a chat's composer and not sent, kept on the hub so it's there after the app restarts,
/// and in every client. It's the person's rather than the chat's, which a hub with several people would keep for each.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    /// What's typed, which can be empty when files are attached.
    #[serde(default)]
    pub text: String,
    /// The files waiting to go with it, in the order they were attached, as the hub has them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Attachment>,
}

impl Draft {
    /// Nothing to keep. Spaces alone are something, since they're what was typed, though they're no prompt.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.files.is_empty()
    }
}

/// What a client asks the hub to keep as a chat's [`Draft`]: what's typed, and the files waiting with it, by ID. Each
/// file is added to the session first, as a prompt's are, so the hub names each as it has it, and leaves out any it
/// hasn't been given.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftToKeep {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<AttachmentId>,
}

impl DraftToKeep {
    /// Only `text`, with no files.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), files: Vec::new() }
    }
}

/// A comment being written on a workspace's diff and not saved, or a saved one being changed, kept on the hub so it's
/// there after the app restarts, and in every client. A workspace has one at a time, since its diff has one box to
/// write in. Like a chat's draft, it's the person's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommentDraft {
    /// The changed line it's on, as a saved comment says where it is.
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    /// The saved comment it changes, rather than adding one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editing: Option<CommentId>,
    #[serde(default)]
    pub text: String,
}

/// What's chosen and typed in answer to the request a chat's agent is waiting on, and not sent, kept on the hub so it's
/// there after the app restarts, and in every client. Like a chat's draft, it's the person's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DecisionDraft {
    /// The request it answers, so what was typed for one that's gone is never taken for the next.
    pub approval_id: ApprovalId,
    pub so_far: SoFar,
}

impl DecisionDraft {
    /// Whether anything's chosen or typed, since a draft with nothing is the same as none.
    pub fn says_anything(&self) -> bool {
        match &self.so_far {
            SoFar::Use { chosen, words, writing: _ } => chosen.is_some() || !words.is_empty(),
            SoFar::Answers { answers } => {
                answers.iter().any(|so_far| !so_far.picked.is_empty() || !so_far.typed.is_empty())
            }
            SoFar::Unknown => false,
        }
    }
}

impl DecisionDraft {
    /// It with what's typed for each of `request`'s secret questions taken out, as the hub keeps it, since a secret
    /// answer reaches the agent and nothing else.
    #[must_use]
    pub fn without_secrets(&self, request: &crate::capability::approvals::Request) -> DecisionDraft {
        let mut draft = self.clone();
        if let SoFar::Answers { answers } = &mut draft.so_far {
            for at in request.secret_questions() {
                if let Some(answer) = answers.get_mut(at) {
                    *answer = AnswerSoFar::default();
                }
            }
        }
        draft
    }
}

/// What's chosen and typed so far, for the kind of request it answers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "drafts", rename_all = "snake_case")]
pub enum SoFar {
    /// Whether to allow a use the agent asks to make, such as a call, and the words to deny it in.
    Use {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chosen: Option<UseChoice>,
        /// What's typed to deny it in the person's own words.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        words: String,
        /// Whether the words are the answer, rather than what's chosen, as they are once the person starts typing.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        writing: bool,
    },
    /// What's chosen and typed on each of the questions asked together, in the order they were asked.
    Answers {
        #[serde(default)]
        answers: Vec<AnswerSoFar>,
    },
    #[serde(other)]
    Unknown,
}

/// Whether a use is allowed or denied.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UseChoice {
    Allow,
    Deny,
    #[serde(other)]
    Unknown,
}

/// What's chosen and typed on one question.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AnswerSoFar {
    /// The options chosen, by their place among the question's options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub picked: Vec<u32>,
    /// What's typed in the row for another answer.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub typed: String,
    /// Whether what's typed counts as an answer. Where several answers are allowed, it can be taken back and kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub other: bool,
}
