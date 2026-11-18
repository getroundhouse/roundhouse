use std::collections::{HashMap, VecDeque};

use protocol::capability::approvals::{ApprovalMode, Request};
use protocol::model::{ApprovalId, Seq, SubagentId, ToolUseId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::Launch;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub(super) struct ThreadId(pub(super) String);

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub(super) struct TurnId(pub(super) String);

pub(super) struct Waiting {
    pub(super) request: Request,
    pub(super) native: NativeRequest,
    /// The subagent that asks, or `None` for the agent itself.
    pub(super) parent: Option<SubagentId>,
}

#[derive(Default)]
pub(super) struct Work {
    /// Native terminal session IDs for unfinished foreground calls, never OS PIDs.
    pub(super) terminals: HashMap<ToolUseId, String>,
    pub(super) calls: HashMap<ToolUseId, protocol::capability::calls::Call>,
    /// The line that started each web search, until it completes, since only then does Codex say what it searched for,
    /// so its call is recorded then, from this line.
    pub(super) searches: HashMap<ToolUseId, String>,
}

pub(super) enum NativeRequest {
    Command(Value),
    File(Value),
    Questions { id: Value, keys: Vec<String> },
    Plan,
}

impl NativeRequest {
    pub(super) fn id(&self) -> Option<&Value> {
        match self {
            Self::Command(id) | Self::File(id) | Self::Questions { id, .. } => Some(id),
            Self::Plan => None,
        }
    }
}

/// How a root turn ended, as Codex said, held while a cancel waits for it, so a turn that finished as it was cancelled
/// is recorded as it finished.
pub(super) struct Ended {
    pub(super) outcome: protocol::event::TurnOutcome,
    pub(super) error: Option<String>,
    pub(super) raw: String,
}

/// A compaction someone asked for, which Codex carries out in a turn of its own, with no message of the person's, so
/// its turn starting is what takes it.
pub(super) struct AskedCompaction {
    /// Where it's recorded.
    pub(super) asked: Seq,
    /// The turn Codex compacts in, once it has started.
    pub(super) turn: Option<TurnId>,
    /// The turn's compaction has come.
    pub(super) answered: bool,
}

pub(super) struct Conversation {
    pub(super) thread: Option<ThreadId>,
    pub(super) turn: Option<TurnId>,
    /// The root turn that ended last. It's set under the same lock that clears `turn`, so a turn that `turn/start`
    /// answered with, but that ended before its caller got the answer, isn't taken back as the active one.
    pub(super) last_ended: Option<TurnId>,
    /// The end of the turn a cancel is waiting for, which the cancel records.
    pub(super) held_end: Option<Ended>,
    pub(super) settings: Launch,
    /// Settings accepted while working belong to the next turn, not the one finishing.
    pub(super) turn_mode: Option<ApprovalMode>,
    pub(super) untaken: VecDeque<Seq>,
    /// The compaction asked for, from when it's sent until the turn Codex compacts in ends.
    pub(super) compaction: Option<AskedCompaction>,
    /// A native intake call can cross a turn boundary; the old turn cannot settle this prompt.
    pub(super) sending: Option<Seq>,
    pub(super) waiting: HashMap<ApprovalId, Waiting>,
    pub(super) work: HashMap<Option<SubagentId>, Work>,
    pub(super) subagents: super::subagents::Subagents,
    pub(super) plan: Option<String>,
    pub(super) reply: Option<String>,
    pub(super) cancelling: bool,
}

impl Conversation {
    pub(super) fn new(settings: Launch) -> Self {
        Self {
            thread: None,
            turn: None,
            last_ended: None,
            held_end: None,
            settings,
            turn_mode: None,
            untaken: VecDeque::new(),
            compaction: None,
            sending: None,
            waiting: HashMap::new(),
            work: HashMap::new(),
            subagents: super::subagents::Subagents::default(),
            plan: None,
            reply: None,
            cancelling: false,
        }
    }
    /// Whether the agent itself, rather than one of its subagents, has a request open, which a new turn of its own
    /// would leave unanswered. A subagent's runs on its own thread, so it holds up nothing of the agent's.
    pub(super) fn asking(&self) -> bool {
        self.waiting.values().any(|waiting| waiting.parent.is_none())
    }
    pub(super) fn work(&mut self, parent: Option<&SubagentId>) -> &mut Work {
        self.work.entry(parent.cloned()).or_default()
    }
    pub(super) fn take_plan(&mut self) -> Option<String> {
        let mode = self.turn_mode.take();
        self.plan.take().or_else(|| {
            mode.is_some_and(|mode| mode.plan)
                .then(|| self.reply.take())
                .flatten()
                .filter(|text| !text.trim().is_empty())
                .map(plan_text)
        })
    }
}

fn plan_text(reply: String) -> String {
    if let Some((_, rest)) = reply.split_once("<proposed_plan>")
        && let Some((plan, _)) = rest.split_once("</proposed_plan>")
    {
        return plan.trim().to_owned();
    }
    reply
}

#[cfg(test)]
mod tests {
    use super::Conversation;
    use crate::codex::claims::{full_access, plan};
    use protocol::model::AgentKind;
    #[test]
    fn next_turn_settings_do_not_turn_a_reply_into_a_plan_or_release_a_plan() {
        let mut state =
            Conversation::new(crate::registry::on(AgentKind::Codex, super::super::one_off::ONE_OFF_MODEL).unwrap());
        state.turn_mode = Some(full_access());
        state.settings.mode = plan();
        state.reply = Some("Done".into());
        assert!(state.take_plan().is_none());
        assert_eq!(state.reply.as_deref(), Some("Done"));
        state.turn_mode = Some(plan());
        state.settings.mode = full_access();
        state.reply = Some("<proposed_plan>Do this</proposed_plan> Please approve".into());
        assert_eq!(state.take_plan().as_deref(), Some("Do this"));
    }
}
