use super::super::{
    AccountId, AnswerDraft, ApprovalId, CACHE_WORTH_TELLING_TOKENS, ContextUsage, Draft, Effort, ModelName,
    PermissionMode, PromptCache, QueuedPrompt, Rev, Seq, SessionId, SessionSettings, Title, TitleSource, Wait,
    WorkspaceId,
};
use crate::event::{AgentTask, TaskKind, TurnOutcome};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    #[serde(other)]
    Unknown,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Unknown => "unknown",
        }
    }
}

/// Where a session's agent has got to. Its methods group the statuses by what they say of the agent, and each grouping
/// that more than one place needs is one of them. `Unknown`, a status from a newer peer, is in none: each grouping
/// claims something of what the agent is doing, and this build can't tell what a status it doesn't know claims, so
/// whatever reads one shows nothing in particular.
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
    /// Whether the agent is on a turn: working, or waiting on someone to answer or approve. Work it left in the
    /// background isn't one, and when that finishes and it picks up again by itself, that's a new turn.
    pub fn in_turn(self) -> bool {
        match self {
            SessionStatus::Running | SessionStatus::WaitingApproval => true,
            SessionStatus::Idle | SessionStatus::Interrupted | SessionStatus::Background | SessionStatus::Unknown => {
                false
            }
        }
    }

    /// Whether the agent is at work, in a turn or in the background, so stopping it would cut something short.
    pub fn is_active(self) -> bool {
        match self {
            SessionStatus::Running | SessionStatus::WaitingApproval | SessionStatus::Background => true,
            SessionStatus::Idle | SessionStatus::Interrupted | SessionStatus::Unknown => false,
        }
    }

    /// Whether the agent's only work, with `background` running there, is waiting in the background on its waits on
    /// the pull request, which the hub keeps.
    pub fn only_waits(self, background: &[AgentTask]) -> bool {
        self == SessionStatus::Background
            && !background.is_empty()
            && background.iter().all(|task| task.kind == TaskKind::Wait)
    }

    /// Whether the hub counts the agent as running, with `background` running there, since stopping the hub now would
    /// cut short what it's doing, so a hub asked to stop only if nothing would notice, as to update it, carries on for
    /// it, and the app counts it as what the update waits on. Waits don't, since the next hub keeps them and starts the
    /// agent again once one ends, though the agent is at work meanwhile, and keeps the machine awake, so its pull
    /// request goes on being read.
    pub fn counts_as_running(self, background: &[AgentTask]) -> bool {
        self.is_active() && !self.only_waits(background)
    }

    /// Whether the agent is working on a turn, rather than waiting on someone to answer or approve, or between turns.
    /// The chat ends with the working line only then, and only then do the agent's requests keep the cache warm.
    pub fn is_working(self) -> bool {
        match self {
            SessionStatus::Running => true,
            SessionStatus::Idle
            | SessionStatus::WaitingApproval
            | SessionStatus::Interrupted
            | SessionStatus::Background
            | SessionStatus::Unknown => false,
        }
    }

    /// Whether the agent has ended its turn and is still there for the next one: idle, or with work going in the
    /// background. A turn ends by coming to one of these, and a prompt the agent takes from one starts the next. An
    /// agent that went away mid-turn isn't between turns, since its turn never ended.
    pub fn between_turns(self) -> bool {
        match self {
            SessionStatus::Idle | SessionStatus::Background => true,
            SessionStatus::Running
            | SessionStatus::WaitingApproval
            | SessionStatus::Interrupted
            | SessionStatus::Unknown => false,
        }
    }

    /// Whether the agent has nothing left to do, so nothing happens until someone sends it something: it's idle, or
    /// went away mid-turn. Work in the background isn't done, since the agent picks up again by itself when that
    /// finishes, so a queued prompt waits for it.
    pub fn is_done(self) -> bool {
        match self {
            SessionStatus::Idle | SessionStatus::Interrupted => true,
            SessionStatus::Running
            | SessionStatus::WaitingApproval
            | SessionStatus::Background
            | SessionStatus::Unknown => false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub agent: AgentKind,
    /// What the run that's running claims, from when it says until it exits, and nothing when no agent is running, as
    /// when a hub starts and doesn't find the chat's agent. A client offers what these say, and what the chat's kind
    /// claims when there are none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_capabilities: Option<super::super::RunCapabilities>,
    pub model: ModelName,
    /// The agent's approval mode, as it last reported it or was told.
    pub permission_mode: PermissionMode,
    /// How hard the agent thinks.
    pub effort: Effort,
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
    /// How that turn ended, as its end said. `None` until a turn has ended, for one that ended without saying, and from
    /// a hub that doesn't keep it. It moves with `reply_seq`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_outcome: Option<TurnOutcome>,
    /// When the newest turn started, by the hub's clock. `None` until one has, and from a hub that doesn't keep it.
    /// It moves with a change of status, so the feed announces it.
    #[serde(default)]
    pub turn_started_at_ms: Option<i64>,
    /// When the person last sent the agent something, by the hub's clock: a prompt, as it's sent or as it's queued
    /// for when the agent finishes, rather than when the hub sends it from the queue, or an answer to a question or
    /// an approval. `None` until then, and from a hub that doesn't keep it. Each moves the record, so the feed
    /// announces it. [`Session::last_active_at_ms`] says when the chat was last worked in from it.
    #[serde(default)]
    pub user_active_at_ms: Option<i64>,
    /// How far the person has read, so a reply past it is unread. It's theirs rather than the session's, which a
    /// hub with several people would keep for each.
    #[serde(default)]
    pub read_seq: Seq,
    /// What the agent is running in the background, as its newest list says. Empty once the agent has gone, since it
    /// takes that work with it.
    #[serde(default)]
    pub background_tasks: Vec<AgentTask>,
    /// Oldest open wait, for clients predating `waits`. New clients use `open_waits`, which distinguishes an omitted
    /// collection from an authoritative empty one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<Wait>,
    /// All open waits. None means an older hub; an empty list means this hub has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits: Option<Vec<Wait>>,
    /// How full the agent's context is, as its newest report said. `None` until it has said, and from a hub that
    /// doesn't keep it. It stays once the agent has gone, since the conversation it resumes is as full.
    #[serde(default)]
    pub context: Option<ContextUsage>,
    /// What the API's cache holds of the conversation, as the agent's newest request left it. `None` before the agent
    /// has asked its model anything, after it compacted, since what the cache held is no longer the conversation, and
    /// from a hub that doesn't keep it. It moves with each of the agent's own requests, which the feed doesn't announce,
    /// so a client has the newest once the turn has ended, when the record is announced. It stays once the agent has
    /// gone, since the cache doesn't go with it.
    #[serde(default)]
    pub prompt_cache: Option<PromptCache>,
    /// The account the agent's newest launch runs under, as the worker said when it launched, or for a login the hub
    /// gave it, as the hub's own record of that says, and the record of it says who that is and what's left of its
    /// limits. `None` until an agent has launched, when nobody was logged in or the login can't be told from another,
    /// while the worker is asked about a launch the hub gave no login after one it gave one, and from a hub that
    /// doesn't keep it. It stays once the agent has gone, as the context does.
    #[serde(default)]
    pub account: Option<AccountId>,
    /// Prompts waiting for the agent to finish, oldest first, as [`QueuedPrompt`] says.
    #[serde(default)]
    pub queued: Vec<QueuedPrompt>,
    /// The queue waits for the person, as [`QueuedPrompt`] says.
    #[serde(default)]
    pub queue_held: bool,
    /// What's typed and attached in the chat's composer and not sent, as [`Draft`] says. Empty from a hub that doesn't
    /// keep it, and once the chat is closed.
    #[serde(default, skip_serializing_if = "Draft::is_empty")]
    pub draft: Draft,
    /// What's chosen and typed in answer to the question its agent is waiting on, as [`AnswerDraft`] says. One for a
    /// question that has gone means nothing. `None` from a hub that doesn't keep it, and once the chat is closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_draft: Option<AnswerDraft>,
    /// The approvals the agent is waiting on, oldest first, changed in the same commit as the event that changes them.
    /// `None` from a hub that doesn't say, whose client finds them among the events it has loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_approvals: Option<Vec<OpenApproval>>,
    /// The prompts the agent has yet to take or drop, oldest first, by where each is recorded, a compaction asked for
    /// included, changed in the same commit as the event that changes them. Until the agent has taken them all, the
    /// queue doesn't send and a compaction is refused. `None` from a hub that doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub untaken: Option<Vec<Seq>>,
    /// Whether the agent has opened its conversation, so there's something to compact. `None` from a hub that doesn't
    /// say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_opened: Option<bool>,
    #[serde(default)]
    pub rev: Rev,
    #[serde(default)]
    pub title_source: TitleSource,
}

/// An approval the agent is waiting on: where its request is in the log, and its ID.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenApproval {
    pub seq: Seq,
    pub approval_id: ApprovalId,
}

impl Session {
    /// Whether the hub would refuse to ask the agent to compact now, as far as the record says: while the agent is on a
    /// turn, has a prompt it hasn't taken, or hasn't opened its conversation. The hub has the last word, and a hub
    /// that doesn't say what the agent has yet to take, or whether it has opened, is refused only for the turn.
    pub fn compaction_waits(&self) -> bool {
        let untaken = self.untaken.as_ref().is_some_and(|untaken| !untaken.is_empty());
        self.status.in_turn() || untaken || self.agent_opened == Some(false)
    }

    /// What it runs with.
    pub fn settings(&self) -> SessionSettings {
        SessionSettings { model: self.model.clone(), permission_mode: self.permission_mode, effort: self.effort }
    }

    /// The agent has replied since the person last read this chat.
    pub fn unread(&self) -> bool {
        self.reply_seq > self.read_seq
    }

    /// Whether the agent stopped short, so the chat needs the person until a turn starts again: it went away mid-turn,
    /// or its newest turn failed.
    pub fn stopped_short(&self) -> bool {
        self.status == SessionStatus::Interrupted || self.turn_failed()
    }

    /// Whether the newest turn failed, as when the model's API refused it for a limit, and no turn has started since. A
    /// turn that has started since hasn't ended, so the reply is still the failed one's, and the status says it's past.
    pub fn turn_failed(&self) -> bool {
        self.reply_outcome == Some(TurnOutcome::Error) && self.status.between_turns()
    }

    /// A turn has ended, so the agent has a reply, read or not.
    pub fn has_reply(&self) -> bool {
        self.reply_seq != Seq::default()
    }

    /// Whether the chat is open, in its workspace's tabs, rather than closed, which stopped its agent. Nothing can be
    /// sent to a closed one until it's reopened.
    pub fn is_open(&self) -> bool {
        self.closed_at_ms.is_none()
    }

    /// What the cache holds of the conversation while it can expire: the agent isn't working, which keeps it warm,
    /// though it can be waiting on the person. A status this build doesn't know might be working, and claims nothing,
    /// as [`SessionStatus`] says, so this says nothing. The newest turn's card says how long it has left from this.
    pub fn idle_cache(&self) -> Option<&PromptCache> {
        let can_expire = match self.status {
            SessionStatus::Idle
            | SessionStatus::WaitingApproval
            | SessionStatus::Interrupted
            | SessionStatus::Background => true,
            SessionStatus::Running | SessionStatus::Unknown => false,
        };
        self.prompt_cache.as_ref().filter(|_| can_expire)
    }

    /// The same, when it's worth telling the person anywhere else: the chat is open, and its context is big enough
    /// that writing it again costs something. [`PromptCache::state`] says how warm it is at a moment.
    pub fn cache_worth_telling(&self) -> Option<&PromptCache> {
        let big = self.context.as_ref().is_some_and(|context| context.used_tokens >= CACHE_WORTH_TELLING_TOKENS);
        self.idle_cache().filter(|_| big && self.is_open())
    }

    /// When the person last worked in this chat, by the hub's clock: when they made it, or last sent its agent
    /// something, as `user_active_at_ms` says. Only what they did counts, so it doesn't move as the agent works or
    /// replies. A hub too old to keep that keeps when the newest turn started, which a prompt mostly starts.
    pub fn last_active_at_ms(&self) -> i64 {
        let sent = self.user_active_at_ms.or(self.turn_started_at_ms);
        sent.map_or(self.created_at_ms, |sent| sent.max(self.created_at_ms))
    }
}

impl Session {
    /// The hub's complete list, falling back only when an older hub omitted it.
    pub fn open_waits(&self) -> &[Wait] {
        self.waits.as_deref().unwrap_or(self.wait.as_slice())
    }
}
