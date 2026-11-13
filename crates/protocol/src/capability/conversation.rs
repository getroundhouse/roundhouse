//! Keeping a conversation across processes, copying one, taking one back, and taking over one started elsewhere.

use serde::{Deserialize, Serialize};

use super::area::area;
use super::{Capabilities, Claimed};
use crate::model::{AgentSessionId, Seq, ids};

area! { "a conversation";
    /// A conversation outlives the process that held it. A launch that resumes it carries on with everything it held,
    /// its model's context included, and reports the same conversation ID in its first `AgentInit`. One the agent can't
    /// find is started afresh, under a new ID, and the adapter says so with a warning, so nobody thinks the agent
    /// remembers what it doesn't.
    Resume => resume: Resume,
    /// A launch can start a new conversation as a copy of another, as it stood before a given prompt or as it stands,
    /// leaving that one as it is. The hub makes the copy a session of its own, whose log starts with the other's up to
    /// that point, and launches its agent with [`Start::Fork`]. The copy has an ID of its own, which the hub gives it
    /// or the agent reports, as the claim's [`Resume::ids`] say, so whatever claims this claims `Resume` too, and
    /// neither changes the worktree.
    Fork => fork: Claimed,
    /// The conversation can be taken back to before a given prompt, as if that prompt and everything after it had never
    /// been sent, and the adapter reports the rewind once it's done, which the hub records, so a client shows what the
    /// agent no longer has. What happens to the files is [`Rewind::files`], and a rewind that puts them back reports
    /// each one.
    Rewind => rewind: Rewind,
    /// A rewind can be undone until the next prompt, which makes it final: the conversation comes back as it was, and
    /// so do the files the rewind put back, and the adapter reports it once it's done.
    UndoRewind => undo_rewind: Claimed,
    /// The files the agent changed since a given prompt can be put back as they were before it, from the agent's own
    /// record of them, leaving the conversation as it is. The adapter reports each file it put back. A file the agent
    /// didn't change is left alone, whoever changed it since.
    RestoreFiles => restore_files: Claimed,
    /// What putting files back would change can be read before anything is put back: the files it would put back,
    /// and how many lines they'd gain and lose in all, as Claude's dry run says.
    PreviewRestoreFiles => preview_restore_files: Claimed,
    /// The conversations the agent holds that Roundhouse didn't start, such as one from its own interface in the
    /// worktree, can be listed, and a launch can take one over with [`Start::Import`]. The hub makes it a session, and
    /// the adapter records its history first, as the agent recorded it, so the log holds all of it.
    Import => import: Claimed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resume {
    pub ids: ConversationIds,
}

/// Who chooses a conversation's ID, which says when the hub can first resume it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationIds {
    /// The hub chooses it and the agent takes it, as Claude's `--session-id` does, so the hub knows it before the agent
    /// has said anything, and can resume a conversation whose first launch failed.
    Given,
    /// The agent chooses it, as Codex's threads and opencode's sessions do, and reports it in its first `AgentInit`.
    /// Until that's recorded there's nothing to resume, and a launch after one that never said starts afresh.
    Chosen,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rewind {
    pub files: RewindFiles,
}

/// What a rewind does to the files the agent changed in the turns it takes back, which a client says before the person
/// rewinds.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RewindFiles {
    /// They stay as they are, as Codex's and Claude's do. [`Capability::RestoreFiles`] puts them back, where it's
    /// claimed.
    Kept,
    /// They're put back as they were before the prompt, with the conversation, as opencode's are, from the snapshots it
    /// takes at every step.
    Restored,
    #[serde(other)]
    Unknown,
}

ids!(
    /// What the agent calls the place in its conversation where a prompt starts, such as Claude's message UUID, Codex's
    /// turn ID or opencode's message ID, which a fork, a rewind and putting files back go by. The adapter records it
    /// as the agent takes the prompt, on its `PromptTaken`, and the hub keeps it with the prompt, so a later launch,
    /// or a worker on another machine, can name the place without having seen it.
    AgentPoint,
);

/// A prompt, by where the log records it and by what the agent calls the place it starts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PromptPoint {
    pub prompt: Seq,
    pub point: AgentPoint,
}

/// How a launch starts its conversation. It takes the place of `AgentSpec`'s `agent_session_id` and `resume`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "start", rename_all = "snake_case")]
pub enum Start {
    /// A new conversation, under the ID the hub gives it where its kind's IDs are [`ConversationIds::Given`].
    New {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conversation: Option<AgentSessionId>,
    },
    /// Carrying on with `conversation`, as [`Capability::Resume`] says.
    Resume { conversation: AgentSessionId },
    /// A copy of `from`, as it stood before the prompt at `before`, or all of it for `None`, as [`Capability::Fork`]
    /// says, under the ID `conversation` where the kind's IDs are [`ConversationIds::Given`].
    Fork {
        from: AgentSessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conversation: Option<AgentSessionId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<AgentPoint>,
    },
    /// Taking over `conversation`, which Roundhouse didn't start, as [`Capability::Import`] says.
    Import { conversation: AgentSessionId },
}

impl Start {
    /// The capability it needs, or `None` for a new conversation, which every agent starts.
    pub fn needs(&self) -> Option<super::Capability> {
        match self {
            Start::New { .. } => None,
            Start::Resume { .. } => Some(Capability::Resume.into()),
            Start::Fork { .. } => Some(Capability::Fork.into()),
            Start::Import { .. } => Some(Capability::Import.into()),
        }
    }

    /// Whether an agent of a kind that claims `claims` can be launched so: the capability it needs, and an ID for a
    /// new conversation exactly where the kind's IDs are [`ConversationIds::Given`].
    pub fn allowed(&self, claims: &Capabilities) -> bool {
        let given = claims.conversation.resume.is_some_and(|resume| resume.ids == ConversationIds::Given);
        let named = match self {
            Start::New { conversation } | Start::Fork { conversation, .. } => conversation.is_some() == given,
            Start::Resume { .. } | Start::Import { .. } => true,
        };
        named && self.needs().is_none_or(|needs| claims.has(needs))
    }
}

/// A conversation the agent holds that Roundhouse didn't start, as [`Capability::Import`] lists them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Outside {
    pub conversation: AgentSessionId,
    /// What the agent calls it, where it names its conversations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// When it last changed, by the worker's clock, which a client only sorts by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_at_ms: Option<i64>,
}
