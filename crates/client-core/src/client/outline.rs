//! The calls for what a chat's outline lists: its prompts and its compactions, a line each, so it can show every one
//! without reading the whole log.

use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{CompactionLine, PromptLine, SessionId};

use super::{Client, Result, unexpected};

impl Client {
    /// Every prompt in the session, oldest first, as a line each. A hub older than this refuses it as unsupported.
    pub fn session_prompt_list(&self, id: &SessionId) -> Result<Vec<PromptLine>> {
        let reply = self.call(ClientRequest::SessionPromptList { id: id.clone() })?;
        let ClientResponse::Prompts { prompts } = reply else {
            return unexpected(&reply);
        };
        Ok(prompts)
    }

    /// Every compaction of the agent's own conversation that compacted, oldest first, as a line each. A hub older than
    /// this refuses it as unsupported.
    pub fn session_compaction_list(&self, id: &SessionId) -> Result<Vec<CompactionLine>> {
        let reply = self.call(ClientRequest::SessionCompactionList { id: id.clone() })?;
        let ClientResponse::Compactions { compactions } = reply else {
            return unexpected(&reply);
        };
        Ok(compactions)
    }
}
