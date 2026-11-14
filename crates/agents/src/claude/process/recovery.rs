//! One correction of a conversation flag, whether the CLI refuses before verification or after a prompt.

use crate::AgentOutput;
use protocol::event::{EventBody, NoticeLevel};
use tokio::sync::mpsc;

#[derive(Default)]
pub(super) struct Recovery {
    corrected: bool,
}

impl Recovery {
    pub(super) async fn correct(&mut self, stderr: &[String], output: &mpsc::Sender<AgentOutput>) -> Option<bool> {
        if self.corrected {
            return None;
        }
        let failure = classify_launch_failure(stderr)?;
        self.corrected = true;
        let resume = matches!(failure, LaunchFailure::AlreadyExists);
        if !resume {
            let notice = EventBody::Notice { level: NoticeLevel::Warning,
                message: "Claude's saved conversation for this session was not found, so it started a new one. The history here is intact, but the agent can't see it.".into(),
                parent_tool_use_id: None };
            let _ = output.send(AgentOutput::event(notice, None)).await;
        }
        Some(resume)
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum LaunchFailure {
    NoConversation,
    AlreadyExists,
}

pub(super) fn classify_launch_failure(stderr: &[String]) -> Option<LaunchFailure> {
    stderr.iter().find_map(|line| {
        if line.contains("No conversation found") {
            Some(LaunchFailure::NoConversation)
        } else if line.contains("is already in use") {
            Some(LaunchFailure::AlreadyExists)
        } else {
            None
        }
    })
}
