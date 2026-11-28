//! A session whose agent is still running on a worker the hub has reached again, whether an earlier hub left it or its
//! connection to this hub dropped: it takes the agent over.

use std::sync::Arc;

use protocol::model::RunId;

use super::had_servers::HadServers;
use super::live::{AgentRun, CarriedOver, LiveSession};
use crate::tool_servers::ServersAsSent;

impl LiveSession {
    /// Has a session that was made before its agent's worker dialed the hub carry on with `carried`, as one made after
    /// does, as a workspace whose worker dialed late has each of its sessions do. A client could have followed the
    /// session meanwhile, which makes it, with no agent, and what it recorded since is what the agent finds. One whose
    /// agent is on a worker that has dialed again has it go on over the new connection. The
    /// `capabilities` are what the session's record says its run claims now. Does
    /// nothing to one that has an agent already.
    pub(crate) fn carry_on_with(
        self: &Arc<Self>,
        carried: CarriedOver,
        capabilities: Option<protocol::model::RunCapabilities>,
    ) {
        let CarriedOver { link, run, recorded, untaken } = carried;
        {
            let mut state = self.state.lock();
            if let Some(agent) = &mut state.agent {
                // Its worker dialed again after its connection dropped, so the same run is on a new link, which is the
                // one its events come on from now. A run that's another is one this session has moved on to.
                let rebound = agent.run == run && agent.link != link;
                if rebound {
                    agent.link = link;
                }
                drop(state);
                if rebound {
                    self.ask_for_reconnects();
                }
                return;
            }
            state.agent = Some(AgentRun { link, run });
            state.capabilities = capabilities;
            state.recorded = recorded;
            state.had_servers = Some(HadServers::new(run, ServersAsSent::default()));
            for asked_by in state.progress.approvals.values_mut() {
                asked_by.get_or_insert(run);
            }
            state.progress.queue.set_untaken(untaken);
        }
        self.carried_on(run);
    }

    /// Does for `run`, which an earlier hub left running, what the last hub may not have.
    pub(super) fn carried_on(self: &Arc<Self>, run: RunId) {
        // So what the agent hears of its account's limits is filed under the account, as for one this hub launched.
        self.launched(run);
        // A tool server may have been signed in to or edited as the last hub went.
        self.ask_for_reconnects();
        // The last hub's wait for the agent to pick up again went with it.
        if self.state.lock().progress.waits_for_pickup() {
            self.wait_for_pickup();
        }
    }
}
