//! A session: one conversation with an agent in a workspace, and what it runs with.

use serde::{Deserialize, Serialize};

use super::{Effort, QueuedPrompt, Rev, Seq, SessionId, SessionOptions, Title, TitleSource, WorkspaceId};
use crate::event::AgentTask;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    #[serde(other)]
    Unknown,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Unknown => "unknown",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Nothing is happening, and the next prompt starts a turn.
    Idle,
    Running,
    WaitingApproval,
    /// The agent went away mid-turn. The next prompt resumes the conversation.
    Interrupted,
    /// The turn has ended, and work the agent started in the background, such as a command, is still going. The
    /// agent picks up again by itself when it finishes, and a prompt starts a turn meanwhile.
    Background,
    #[serde(other)]
    Unknown,
}

impl SessionStatus {
    /// Whether the agent is at work, in a turn or in the background, so stopping it would cut something short.
    pub fn is_active(self) -> bool {
        match self {
            SessionStatus::Running | SessionStatus::WaitingApproval | SessionStatus::Background => true,
            SessionStatus::Idle | SessionStatus::Interrupted | SessionStatus::Unknown => false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub agent: AgentKind,
    pub model: Option<String>,
    /// Passed through to the agent. `None` leaves the choice to the user's own agent settings.
    pub permission_mode: Option<String>,
    /// How hard the agent thinks. `None` leaves it to the user's own agent settings.
    #[serde(default)]
    pub effort: Option<Effort>,
    pub status: SessionStatus,
    /// The newest event when this copy was read, and `updated_at_ms` its time. Every event moves them,
    /// and the feed doesn't announce that, or its revision, so the log says how far a session has got.
    pub last_seq: Seq,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// What the chat is called. `None` until its first prompt or a rename.
    #[serde(default, deserialize_with = "Title::deserialize_optional")]
    pub title: Option<Title>,
    #[serde(default)]
    pub closed_at_ms: Option<i64>,
    /// The event that ended the newest turn, which makes what the agent said then its reply. 0 until a turn has
    /// ended. It moves with a change of status, so the feed announces it.
    #[serde(default)]
    pub reply_seq: Seq,
    /// How far the person has read, so a reply past it is unread. It's theirs rather than the session's, which a
    /// hub with several people would keep for each.
    #[serde(default)]
    pub read_seq: Seq,
    /// What the agent is running in the background, as its newest list says. Empty once the agent has gone, since it
    /// takes that work with it.
    #[serde(default)]
    pub background_tasks: Vec<AgentTask>,
    /// Prompts waiting for the agent to finish, oldest first, as [`QueuedPrompt`] says.
    #[serde(default)]
    pub queued: Vec<QueuedPrompt>,
    /// The queue waits for the person, as [`QueuedPrompt`] says.
    #[serde(default)]
    pub queue_held: bool,
    #[serde(default)]
    pub rev: Rev,
    #[serde(default)]
    pub title_source: TitleSource,
}

impl Session {
    /// What it runs with.
    pub fn options(&self) -> SessionOptions {
        SessionOptions { model: self.model.clone(), permission_mode: self.permission_mode.clone(), effort: self.effort }
    }

    /// The agent has replied since the person last read this chat.
    pub fn unread(&self) -> bool {
        self.reply_seq > self.read_seq
    }
}
