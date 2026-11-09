//! Whether the agent has taken a recorded prompt yet.

use protocol::event::{AgentRunState, EventBody};
use protocol::model::Seq;

use super::SessionLog;

#[cfg(test)]
mod tests;

/// Where a recorded prompt stands with the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptState {
    /// Sent mid-turn, and the agent hasn't taken it yet. It takes it once it can, such as when a running tool finishes.
    Waiting,
    Taken,
    /// The agent never took it, because it was stopped first.
    Dropped,
}

impl SessionLog {
    /// Where the prompt recorded at `seq` stands, from what's loaded after it. What the agent says about the prompt
    /// counts first, since the idle and the end of the turn before it can be recorded after it. Without that, a prompt
    /// is taken once the agent ends a turn, goes idle or exits: one that's waiting when a turn ends starts the next at
    /// once, and one a stop drops is dropped before the turn ends. An agent that never says so also takes a prompt
    /// when it starts the turn for it, which it reports. Something after it that isn't loaded is far behind the tail,
    /// and so is anything waiting there.
    pub fn prompt_state(&self, seq: Seq) -> PromptState {
        let mut seems_taken = false;
        let mut says_what_it_takes = false;
        let mut turns_ended = 0;
        for later in seq + 1..=self.last_seq {
            let Some(event) = self.events.get(&later) else { return PromptState::Taken };
            match &event.body {
                EventBody::PromptTaken { prompt } if *prompt == seq => return PromptState::Taken,
                EventBody::PromptDropped { prompt } if *prompt == seq => return self.taken_after_all(seq, later),
                EventBody::PromptTaken { .. } | EventBody::PromptDropped { .. } => says_what_it_takes = true,
                EventBody::AgentInit { .. } if !says_what_it_takes => seems_taken = true,
                // An agent says what it dropped before it exits.
                EventBody::AgentExited { .. } => return PromptState::Taken,
                // The agent says what it did with a prompt by the end of the turn the prompt starts, or the one that
                // drops it, and the first to end can be the one it was sent during.
                EventBody::TurnEnded { .. } => {
                    turns_ended += 1;
                    if turns_ended == 2 {
                        return PromptState::Taken;
                    }
                    seems_taken = true;
                }
                EventBody::AgentState { state: AgentRunState::Idle } => seems_taken = true,
                EventBody::UserPrompt { .. }
                | EventBody::AgentStarted { .. }
                | EventBody::AgentInit { .. }
                | EventBody::AgentState { .. }
                | EventBody::AgentTasks { .. }
                | EventBody::Assistant { .. }
                | EventBody::AgentUser { .. }
                | EventBody::ApprovalRequested { .. }
                | EventBody::ApprovalResolved { .. }
                | EventBody::Notice { .. }
                | EventBody::AgentMode { .. }
                | EventBody::AgentOther { .. }
                | EventBody::Unknown => {}
            }
        }
        if seems_taken { PromptState::Taken } else { PromptState::Waiting }
    }

    /// For a prompt dropped at `dropped`: whether an agent that replaced the one that dropped it took it after all.
    fn taken_after_all(&self, seq: Seq, dropped: Seq) -> PromptState {
        let taken = self
            .events
            .range(dropped + 1..)
            .any(|(_, e)| matches!(e.body, EventBody::PromptTaken { prompt } if prompt == seq));
        if taken { PromptState::Taken } else { PromptState::Dropped }
    }
}
