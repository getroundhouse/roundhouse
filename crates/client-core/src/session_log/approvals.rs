//! The approvals still waiting on an answer, among what a log holds.

use protocol::event::{EventBody, SessionEvent};

use super::SessionLog;

impl SessionLog {
    /// Open approvals among what's loaded. An approval blocks its agent, so an open one is always
    /// near the tail, which is always loaded. Only the events that run unbroken back from the newest
    /// count, since an older request's answer can be in a page that isn't loaded, such as when a page
    /// is fetched for scrolling back.
    pub fn open_approvals(&self) -> Vec<&SessionEvent> {
        let mut open: Vec<&SessionEvent> = Vec::new();
        for event in self.unbroken_from_newest() {
            match &event.body {
                EventBody::ApprovalRequested { .. } => open.push(event),
                EventBody::ApprovalResolved { approval_id, .. } => open.retain(
                    |e| !matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id),
                ),
                EventBody::UserPrompt { .. }
                | EventBody::PromptTaken { .. }
                | EventBody::PromptDropped { .. }
                | EventBody::ToolCallDropped { .. }
                | EventBody::AgentStarted { .. }
                | EventBody::AgentInit { .. }
                | EventBody::AgentState { .. }
                | EventBody::AgentTasks { .. }
                | EventBody::AgentLimits { .. }
                | EventBody::Assistant { .. }
                | EventBody::AgentUser { .. }
                | EventBody::TurnEnded { .. }
                | EventBody::AgentExited { .. }
                | EventBody::Notice { .. }
                | EventBody::AgentMode { .. }
                | EventBody::SubagentStarted { .. }
                | EventBody::SubagentEnded { .. }
                | EventBody::AgentOther { .. }
                | EventBody::Unknown => {}
            }
        }
        open
    }

    /// The loaded events with every event after them loaded too, oldest first.
    fn unbroken_from_newest(&self) -> impl Iterator<Item = &SessionEvent> {
        let mut wanted = self.last;
        let unbroken = |position: &&u64| {
            let next = **position == wanted;
            wanted = position.saturating_sub(1);
            next
        };
        let first = self.events.keys().rev().take_while(unbroken).last().copied();
        first.into_iter().flat_map(|first| self.events.range(first..).map(|(_, event)| event))
    }
}
