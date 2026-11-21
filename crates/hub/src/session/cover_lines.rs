//! What the agent is doing, said for it while it can't answer what was said out loud, which every subscriber's tail
//! has while conversation mode is on, as [`protocol::tail::Covering`] says.

use protocol::capability::output::{CoverLine, CoverWait};
use protocol::model::RunId;

use super::{LiveItem, LiveSession, LiveState, publish};

impl LiveSession {
    /// Shows `line`, said for the agent of `run`, in every subscriber's tail, while the run is the session's and
    /// conversation mode is on, since otherwise nobody is listening for it.
    pub(super) fn covering(&self, run: RunId, line: CoverLine) {
        let mut state = self.state.lock();
        if !is_listened_for(&state, run) {
            return;
        }
        state.tail.covering(run, line);
        publish(&mut state, LiveItem::Rewritten);
    }

    /// The adapter of `run` closed the wait `wait`, which ends its line if it's that wait's.
    pub(super) fn covered(&self, run: RunId, wait: CoverWait) {
        let mut state = self.state.lock();
        let before = state.tail.covering.is_some();
        state.tail.covered(run, wait);
        if before && state.tail.covering.is_none() {
            publish(&mut state, LiveItem::Rewritten);
        }
    }
}

/// Whether a line said for the agent of `run` has anyone to hear it.
fn is_listened_for(state: &LiveState, run: RunId) -> bool {
    state.agent.is_some_and(|agent| agent.run == run) && !state.interaction.is_typing()
}

/// Drops the line said for the agent, since conversation mode is off and nobody is listening for it, and tells every
/// subscriber if there was one.
pub(super) fn nobody_listening(state: &mut LiveState) {
    if state.tail.uncover() {
        publish(state, LiveItem::Rewritten);
    }
}
