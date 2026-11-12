//! A chat's record: which agent it runs, where that agent has got to, and what the person has read.

use serde::{Deserialize, Serialize};

use super::{
    AccountId, AnswerDraft, ApprovalId, CACHE_WORTH_TELLING_TOKENS, ContextUsage, Draft, Effort, ModelName,
    PermissionMode, PromptCache, QueuedPrompt, Rev, Seq, SessionId, SessionSettings, Title, TitleSource, Wait,
    WorkspaceId,
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
    /// The wait on the pull request the agent has open, as the hub keeps it, which a chat has one of at a time. It's
    /// the hub's, so it says how a wait stands where the list of tasks only has the CLI's copy of it, which can lag
    /// behind, and it lasts once the agent has gone. Starting or ending one moves the record, so the feed announces
    /// it. `None` with none open, and from a hub that doesn't keep them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<Wait>,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::model::{CacheTtl, WaitUntil};

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

    /// A chat in `status`, as a hub names it, whose agent's context holds `used_tokens`.
    fn session(status: &str, used_tokens: u64) -> Session {
        let context = ContextUsage {
            used_tokens,
            max_tokens: 1_000_000,
            model_tokens: None,
            max_set_by: None,
            parts: Vec::new(),
        };
        Session {
            status: serde_json::from_value(json!(status)).unwrap(),
            last_seq: 4,
            context: Some(context),
            prompt_cache: Some(PromptCache { used_at_ms: 0, ttl: CacheTtl::OneHour, model: None, tokens: 0 }),
            ..crate::fixtures::session("s")
        }
    }

    /// A hub that doesn't keep waits sends none. One that does sends the open one in this form, which its store's query
    /// writes, with a `call` the CLI didn't say as null.
    #[test]
    fn a_session_from_a_hub_that_doesnt_keep_waits_has_none_and_one_that_does_has_its_open_wait() {
        let mut said = serde_json::to_value(session("background", 0)).unwrap();
        said["wait"] = json!({
            "id": "a", "session_id": "s", "workspace_id": "w", "number": 7, "call": null, "until": "checks",
            "started_at_ms": 1,
        });

        let waiting: Session = serde_json::from_value(said).unwrap();

        assert_eq!(session("idle", 0).wait, None);
        let wait = waiting.wait.unwrap();
        assert_eq!((wait.number, wait.call, wait.until, wait.ended), (7, None, WaitUntil::Checks, None));
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

    /// Each grouping, for every status. One from a newer peer is in none of them.
    #[test]
    fn each_status_is_in_the_groups_that_say_what_its_agent_is_doing() {
        use SessionStatus::{Background, Idle, Interrupted, Running, Unknown, WaitingApproval};
        let groups = |status: SessionStatus| {
            [status.in_turn(), status.is_active(), status.is_working(), status.between_turns(), status.is_done()]
        };

        // In a turn, active, working, between turns, done.
        assert_eq!(groups(Idle), [false, false, false, true, true]);
        assert_eq!(groups(Running), [true, true, true, false, false]);
        assert_eq!(groups(WaitingApproval), [true, true, false, false, false]);
        assert_eq!(groups(Interrupted), [false, false, false, false, true]);
        assert_eq!(groups(Background), [false, true, false, true, false]);
        assert_eq!(groups(Unknown), [false; 5]);
    }

    #[test]
    fn the_turns_card_says_what_the_cache_holds_whatever_the_size_but_not_while_the_agent_works() {
        assert!(session("idle", 9_000).idle_cache().is_some());
        assert!(session("running", 9_000).idle_cache().is_none());
        assert!(session("thinking_hard", 9_000).idle_cache().is_none(), "a status from a newer hub might be working");
    }

    /// As the hub refuses one, so the button to ask is off when asking would be refused.
    #[test]
    fn a_compaction_waits_for_the_turn_for_every_prompt_to_be_taken_and_for_a_conversation() {
        let says = |status, untaken: &[Seq], agent_opened| Session {
            untaken: Some(untaken.to_vec()),
            agent_opened: Some(agent_opened),
            ..session(status, 9_000)
        };
        assert!(!says("idle", &[], true).compaction_waits());
        assert!(says("running", &[], true).compaction_waits());
        assert!(says("idle", &[5], true).compaction_waits(), "a prompt or compaction on its way");
        assert!(says("idle", &[], false).compaction_waits(), "nothing to compact yet");
        assert!(session("running", 9_000).compaction_waits(), "a hub that doesn't say still says when it's working");
        assert!(!session("idle", 9_000).compaction_waits(), "and is asked otherwise");
    }
}
