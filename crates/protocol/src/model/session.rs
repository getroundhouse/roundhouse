//! A chat's record: which agent it runs, where that agent has got to, and what the person has read.

use serde::{Deserialize, Serialize};

use super::{
    AccountId, AnswerDraft, CACHE_WORTH_TELLING_TOKENS, ContextUsage, Draft, Effort, ModelName, PermissionMode,
    PromptCache, QueuedPrompt, Rev, Seq, SessionId, SessionSettings, Title, TitleSource, WorkspaceId,
};
use crate::event::{AgentTask, TaskKind};

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
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub workspace_id: WorkspaceId,
    pub agent: AgentKind,
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
    /// The account the agent's newest launch runs under, as the worker said when it launched, and the record of it
    /// says who that is and what's left of its limits. `None` until an agent has launched, when nobody was logged in
    /// or the login can't be told from another, and from a hub that doesn't keep it. It stays once the agent has
    /// gone, as the context does.
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
    #[serde(default)]
    pub rev: Rev,
    #[serde(default)]
    pub title_source: TitleSource,
}

impl Session {
    /// What it runs with.
    pub fn settings(&self) -> SessionSettings {
        SessionSettings { model: self.model.clone(), permission_mode: self.permission_mode, effort: self.effort }
    }

    /// The agent has replied since the person last read this chat.
    pub fn unread(&self) -> bool {
        self.reply_seq > self.read_seq
    }

    /// What the cache holds of the conversation while it can expire: the agent isn't working, which keeps it warm,
    /// though it can be waiting on the person. A status this build doesn't know might be working, so it says nothing.
    /// The newest turn's card says how long it has left from this.
    pub fn idle_cache(&self) -> Option<&PromptCache> {
        let working = match self.status {
            SessionStatus::Running | SessionStatus::Unknown => true,
            SessionStatus::Idle
            | SessionStatus::WaitingApproval
            | SessionStatus::Interrupted
            | SessionStatus::Background => false,
        };
        self.prompt_cache.as_ref().filter(|_| !working)
    }

    /// The same, when it's worth telling the person anywhere else: the chat is open, and its context is big enough
    /// that writing it again costs something. [`PromptCache::state`] says how warm it is at a moment.
    pub fn cache_worth_telling(&self) -> Option<&PromptCache> {
        let big = self.context.as_ref().is_some_and(|context| context.used_tokens >= CACHE_WORTH_TELLING_TOKENS);
        self.idle_cache().filter(|_| big && self.closed_at_ms.is_none())
    }

    /// When the person last worked in this chat, by the hub's clock: when they made it, or last sent its agent
    /// something, as `user_active_at_ms` says. Only what they did counts, so it doesn't move as the agent works or
    /// replies. A hub too old to keep that keeps when the newest turn started, which a prompt mostly starts.
    pub fn last_active_at_ms(&self) -> i64 {
        let sent = self.user_active_at_ms.or(self.turn_started_at_ms);
        sent.map_or(self.created_at_ms, |sent| sent.max(self.created_at_ms))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::model::CacheTtl;

    /// Waits are the hub's, which the next hub keeps, starting the agent again once one ends, so an agent with only
    /// waits in the background isn't counted as running. One with anything else there, or on a turn, is.
    #[test]
    fn an_agent_with_only_waits_in_the_background_isnt_counted_as_running() {
        use SessionStatus::{Background, Idle, Running, WaitingApproval};
        let task = |id: &str, kind| AgentTask { id: id.into(), description: None, kind, call: None };
        let (wait, other_wait, command) =
            (task("w", TaskKind::Wait), task("x", TaskKind::Wait), task("c", TaskKind::Command));

        assert!(!Background.counts_as_running(std::slice::from_ref(&wait)));
        assert!(!Background.counts_as_running(&[wait.clone(), other_wait]));
        assert!(Background.counts_as_running(&[wait.clone(), command.clone()]));
        assert!(Background.counts_as_running(&[command]));
        assert!(Background.counts_as_running(&[]), "nothing says it only waits");
        for in_a_turn in [Running, WaitingApproval] {
            assert!(in_a_turn.counts_as_running(std::slice::from_ref(&wait)), "{in_a_turn:?}");
        }
        assert!(!Idle.counts_as_running(&[]));
    }

    fn session(status: &str, used_tokens: u64) -> Session {
        let mut session: Session = serde_json::from_value(json!({
            "id": "s", "workspace_id": "w", "agent": "claude", "model": "opus", "permission_mode": "default",
            "effort": "high", "status": status, "last_seq": 4, "created_at_ms": 0, "updated_at_ms": 0,
        }))
        .unwrap();
        session.context = Some(ContextUsage { used_tokens, max_tokens: 1_000_000, parts: Vec::new() });
        session.prompt_cache = Some(PromptCache { used_at_ms: 0, ttl: CacheTtl::OneHour, model: None, tokens: 0 });
        session
    }

    #[test]
    fn a_cache_is_worth_telling_of_once_the_context_is_big_enough_and_the_agent_isnt_working() {
        assert!(session("idle", CACHE_WORTH_TELLING_TOKENS).cache_worth_telling().is_some());
        assert!(session("waiting_approval", 313_500).cache_worth_telling().is_some(), "it can expire while it waits");
        assert!(session("idle", CACHE_WORTH_TELLING_TOKENS - 1).cache_worth_telling().is_none());
        assert!(session("running", 313_500).cache_worth_telling().is_none(), "each request keeps it warm");
        let closed = Session { closed_at_ms: Some(1), ..session("idle", 313_500) };
        assert!(closed.cache_worth_telling().is_none(), "nobody will send a closed chat anything");
    }

    #[test]
    fn the_turns_card_says_what_the_cache_holds_whatever_the_size_but_not_while_the_agent_works() {
        assert!(session("idle", 9_000).idle_cache().is_some());
        assert!(session("running", 9_000).idle_cache().is_none());
        assert!(session("thinking_hard", 9_000).idle_cache().is_none(), "a status from a newer hub might be working");
    }
}
