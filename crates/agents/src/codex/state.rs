use std::collections::{HashMap, VecDeque};

use protocol::capability::approvals::Request;
use protocol::model::{ApprovalId, Seq, SessionSettings, ToolUseId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub(super) struct ThreadId(pub(super) String);

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub(super) struct TurnId(pub(super) String);

pub(super) struct Waiting {
    pub(super) request: Request,
    pub(super) native: NativeRequest,
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

pub(super) struct Conversation {
    pub(super) thread: Option<ThreadId>,
    pub(super) turn: Option<TurnId>,
    pub(super) settings: SessionSettings,
    /// Settings accepted while working belong to the next turn, not the one finishing.
    pub(super) turn_mode: Option<protocol::model::PermissionMode>,
    pub(super) untaken: VecDeque<Seq>,
    /// A native intake call can cross a turn boundary; the old turn cannot settle this prompt.
    pub(super) sending: Option<Seq>,
    pub(super) waiting: HashMap<ApprovalId, Waiting>,
    /// Native terminal session IDs for unfinished foreground calls, never OS PIDs.
    pub(super) terminals: HashMap<ToolUseId, String>,
    pub(super) calls: HashMap<ToolUseId, protocol::capability::calls::Call>,
    pub(super) plan: Option<String>,
    pub(super) reply: Option<String>,
    pub(super) cancelling: bool,
}

impl Conversation {
    pub(super) fn new(settings: SessionSettings) -> Self {
        Self {
            thread: None,
            turn: None,
            settings,
            turn_mode: None,
            untaken: VecDeque::new(),
            sending: None,
            waiting: HashMap::new(),
            terminals: HashMap::new(),
            calls: HashMap::new(),
            plan: None,
            reply: None,
            cancelling: false,
        }
    }
    pub(super) fn take_plan(&mut self) -> Option<String> {
        let mode = self.turn_mode.take();
        self.plan.take().or_else(|| {
            (mode == Some(protocol::model::PermissionMode::Plan))
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
    use protocol::model::PermissionMode;
    #[test]
    fn next_turn_settings_do_not_turn_a_reply_into_a_plan_or_release_a_plan() {
        let mut state = Conversation::new(crate::codex::description().defaults);
        state.turn_mode = Some(PermissionMode::FullAccess);
        state.settings.permission_mode = PermissionMode::Plan;
        state.reply = Some("Done".into());
        assert!(state.take_plan().is_none());
        assert_eq!(state.reply.as_deref(), Some("Done"));
        state.turn_mode = Some(PermissionMode::Plan);
        state.settings.permission_mode = PermissionMode::FullAccess;
        state.reply = Some("<proposed_plan>Do this</proposed_plan> Please approve".into());
        assert_eq!(state.take_plan().as_deref(), Some("Do this"));
    }
}
