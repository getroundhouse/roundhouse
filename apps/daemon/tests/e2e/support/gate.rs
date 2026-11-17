//! A file a test makes to end the fake agent's work, or a script's, when it's done with it at work, rather than after a
//! time.
//! The UI tests have one of their own, until #248 gives the two suites one support.

#![cfg(test)]

use std::fmt;
use std::path::PathBuf;

use super::{Stack, wait_until};

/// What the fake agent's `slow-until:`, `think-until:`, `hang-until:`, `request-until:`, `agent-until:` and
/// `agent-foreground-until:` work until, put in a script as its path, and what a script the hub runs waits for, with
/// [`Gate::shell_wait`]. Their work ends once it's opened.
pub(crate) struct Gate(PathBuf);

impl Gate {
    /// One in the stack's directory, named `name`, so each of a test's gates needs a name of its own.
    pub(crate) fn new(stack: &Stack, name: &str) -> Self {
        Self(stack.path(name))
    }

    pub(crate) fn open(&self) {
        std::fs::write(&self.0, "").unwrap();
    }

    /// A shell line that waits until it's opened, for a script the hub runs, such as a workspace's setup, to keep at
    /// work while the test acts. It gives up once the stack's directory has gone too, so a test that fails before
    /// opening it leaves nothing waiting.
    pub(crate) fn shell_wait(&self) -> String {
        let dir = self.0.parent().map_or_default(|dir| dir.display().to_string());
        format!("until [ -e '{}' ] || [ ! -d '{dir}' ]; do sleep 0.05; done", self.0.display())
    }

    /// Waits until the agent has a prompt sent while it works, which the fake agent says beside the gate, so the prompt
    /// is taken into the turn however soon the work ends. Recorded isn't enough, since the hub sends it on after.
    pub(crate) fn wait_for_a_prompt(&self) {
        let prompted = PathBuf::from(format!("{}.prompted", self.0.display()));
        wait_until("the agent to have the prompt", || prompted.exists());
    }

    pub(crate) fn open_once_the_agent_has_a_prompt(&self) {
        self.wait_for_a_prompt();
        self.open();
    }

    /// How many chunks `slow-until:` or `think-until:` has said and gone on from, which the fake agent counts beside
    /// the gate, so a test waits for it to have said more rather than for a time.
    pub(crate) fn said(&self) -> usize {
        std::fs::read_to_string(format!("{}.said", self.0.display())).map_or(0, |said| said.lines().count())
    }
}

impl fmt::Display for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}
