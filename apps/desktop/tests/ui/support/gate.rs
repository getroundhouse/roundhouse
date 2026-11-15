//! A file a test makes to end the fake agent's work when it's done with the agent at work, rather than after a time.

#![cfg(test)]

use std::fmt;
use std::path::PathBuf;

use super::{Stack, wait_until};

/// What the fake agent's `slow-until:`, `think-until:`, `hang-until:` and `agent-until:` work until, put in a script
/// as its path. Their work ends once it's opened.
pub(crate) struct Gate(PathBuf);

impl Gate {
    /// One in the stack's directory, named `name`, so each of a test's gates needs a name of its own.
    pub(crate) fn new(stack: &Stack, name: &str) -> Self {
        Self(stack.dir.path().join(name))
    }

    pub(crate) fn open(&self) {
        std::fs::write(&self.0, "").unwrap();
    }

    /// Opens it once the agent has a prompt sent while it works, which the fake agent says beside it, so the prompt is
    /// taken into the turn however soon the work ends. Recorded isn't enough, since the hub sends it on after.
    pub(crate) fn open_once_the_agent_has_a_prompt(&self) {
        let prompted = PathBuf::from(format!("{}.prompted", self.0.display()));
        wait_until("the agent to have the prompt", || prompted.exists());
        self.open();
    }
}

impl fmt::Display for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}
