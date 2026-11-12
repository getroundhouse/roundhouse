//! The call that asks what each kind of agent a hub runs is called and offers.

use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::AgentDescription;

use super::{Client, Result, unexpected};

impl Client {
    /// Each kind of agent the hub runs, as its adapter describes it, in the order a client offers them. A hub older
    /// than this refuses it as unsupported. [`Agents::read`](crate::Agents::read) is what a client holds of it.
    pub fn agent_list(&self) -> Result<Vec<AgentDescription>> {
        let reply = self.call(ClientRequest::AgentList)?;
        let ClientResponse::Agents { agents } = reply else {
            return unexpected(&reply);
        };
        Ok(agents)
    }
}
