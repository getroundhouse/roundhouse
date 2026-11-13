//! The call that asks what each kind of agent a hub runs is called and offers.

use protocol::client::admin;
use protocol::model::AgentDescription;

use super::{Client, Result};

impl Client {
    /// Each kind of agent the hub runs, as its adapter describes it, in the order a client offers them. A hub older
    /// than this refuses it as unsupported. [`Agents::read`](crate::Agents::read) is what a client holds of it.
    pub fn agent_list(&self) -> Result<Vec<AgentDescription>> {
        Ok(self.call(admin::AgentList)?.agents)
    }
}
