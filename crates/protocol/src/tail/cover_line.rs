//! What the agent is doing, said for it while it can't answer what was said out loud, which its adapter alone ends.

use serde::{Deserialize, Serialize};

use super::stream::{Tail, later};
use crate::capability::output::{CoverLine, CoverWait};
use crate::model::RunId;

/// The line said for the agent of `run`, as [`CoverLine`] says. It's over when the adapter closes its wait, or the agent
/// exits, and no other rule of the tail's ends it, since only the adapter knows whether the agent has answered. A
/// device tells one line from the next by its run, wait and number.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Covering {
    /// The run it's said for, so an earlier run's exit can't say it's over. The hub sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    pub line: CoverLine,
}

impl Tail {
    /// The adapter of `run` says `line`, in place of any line before it.
    pub fn covering(&mut self, run: RunId, line: CoverLine) {
        self.covering = Some(Box::new(Covering { run: Some(run), line }));
    }

    /// The adapter of `run` closed the wait `wait`, which ends the line if it's that wait's.
    pub fn covered(&mut self, run: RunId, wait: CoverWait) {
        self.covering = self.covering.take().filter(|covering| covering.run != Some(run) || covering.line.wait != wait);
    }

    /// The agent of `run` exited, so nothing will close the wait of its line, or of an earlier run's.
    pub(super) fn uncovered(&mut self, run: Option<RunId>) {
        self.covering = self.covering.take().filter(|covering| later(run, covering.run));
    }

    /// Drops the line, as the hub does once nobody is listening for it.
    pub fn uncover(&mut self) -> bool {
        self.covering.take().is_some()
    }
}
