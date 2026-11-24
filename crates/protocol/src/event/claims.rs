//! Which events say what their run claims.

use super::EventBody;
use crate::capability::Capabilities;

impl EventBody {
    /// What the run that records it says it claims, if it says, which it does on its start.
    pub fn claims(&self) -> Option<&Capabilities> {
        let EventBody::AgentStarted { capabilities, .. } = self else { return None };
        capabilities.as_deref()
    }
}
