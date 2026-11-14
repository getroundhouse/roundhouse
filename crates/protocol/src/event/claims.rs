//! Which events say what their run claims.

use super::EventBody;
use crate::capability::Capabilities;

impl EventBody {
    /// What the run that records it says it claims, if it says: its start, on a worker of this protocol, or its
    /// initialization, on one of protocol 3, whose start doesn't.
    pub fn claims(&self) -> Option<&Capabilities> {
        let (EventBody::AgentStarted { capabilities, .. } | EventBody::AgentInit { capabilities, .. }) = self else {
            return None;
        };
        capabilities.as_deref()
    }
}
