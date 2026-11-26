//! What a view reads of a chat that its agent, or the hub's reads of GitHub, last said, rather than what the hub
//! decides: how full the agent's context is, which tool servers it has, what the API's cache holds of the conversation,
//! and what each of the chat's open waits last read of the pull requests it watches. Each is read from its observation:
//! the context, the servers and the waits' reads at the chat's own level, so only while something follows the chat, and
//! the cache at the hub's.

use protocol::capability::setup::ToolServerState;
use protocol::model::{ContextUsage, Observation, ObservationKey, Rev, Session, SessionId, Wait, WaitRead};

use super::Snapshot;
use super::observed::said;
use crate::prompt_cache::ConversationCache;

impl Snapshot {
    /// How full `session`'s agent's context is, as it last said. `None` until it has, and while nothing follows the
    /// chat.
    pub fn context<'a>(&'a self, session: &'a Session) -> Option<&'a ContextUsage> {
        let key = ObservationKey::SessionContext { session_id: session.id.clone() };
        let Some(Observation::SessionContext { context, .. }) = self.observation(&key) else { return None };
        said(context)
    }

    /// The tool servers `session`'s agent has, as its run last said. `None` until it has, for an agent that says nothing of
    /// them, while none is running, and while nothing follows the chat.
    pub(crate) fn tool_servers_said(&self, session: &SessionId) -> Option<&[ToolServerState]> {
        let key = ObservationKey::SessionToolServers { session_id: session.clone() };
        let Some(Observation::SessionToolServers { servers, .. }) = self.observation(&key) else { return None };
        said(servers).map(Vec::as_slice)
    }

    /// The revision of what it last said of them, for telling whether it has said more since.
    pub(crate) fn tool_servers_rev(&self, session: &SessionId) -> Option<Rev> {
        self.observation(&ObservationKey::SessionToolServers { session_id: session.clone() }).map(Observation::rev)
    }

    /// What the API's cache holds of `session`'s conversation, and whether its context is big enough that writing it
    /// again costs something, which [`crate::prompt_cache`] reads. Holds nothing until the hub has said.
    pub fn conversation_cache<'a>(&'a self, session: &'a Session) -> ConversationCache<'a> {
        let key = ObservationKey::SessionCache { session_id: session.id.clone() };
        let held = self.observation(&key);
        let cache = if let Some(Observation::SessionCache { cache, .. }) = held { said(cache) } else { None };
        ConversationCache {
            held: cache.and_then(|cache| cache.held.as_ref()),
            worth_telling: cache.is_some_and(|cache| cache.worth_telling),
        }
    }

    /// What the open wait `wait` last read of the pull requests it watches, and how its last read went. `None` until
    /// the hub has said, while nothing follows its chat, and for one the singular tool started, which reads nothing.
    pub fn wait_read(&self, wait: &Wait) -> Option<WaitRead> {
        let key = ObservationKey::WaitRead { session_id: wait.session_id.clone(), wait: wait.id.clone() };
        let Some(Observation::WaitRead { read, .. }) = self.observation(&key) else { return None };
        said(read).cloned()
    }
}
