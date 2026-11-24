//! What a view reads of a chat that its agent, or the hub's reads of GitHub, last said, rather than what the hub
//! decides: how full the agent's context is, what the API's cache holds of the conversation, and what each of the
//! chat's waits last read of the pull requests it watches. Each is read here alone, from the chat's record, so that
//! where the hub sends one apart from the record, as an observation, only these change.

use protocol::model::{CACHE_WORTH_TELLING_TOKENS, ContextUsage, Session, Wait, WaitRead};

use super::Snapshot;
use crate::prompt_cache::ConversationCache;

impl Snapshot {
    /// How full `session`'s agent's context is, as it last said. `None` until it has.
    pub fn context<'a>(&'a self, session: &'a Session) -> Option<&'a ContextUsage> {
        session.context.as_ref()
    }

    /// What the API's cache holds of `session`'s conversation, and whether its context is big enough that writing it
    /// again costs something, which [`crate::prompt_cache`] reads.
    pub fn conversation_cache<'a>(&'a self, session: &'a Session) -> ConversationCache<'a> {
        let big = self.context(session).is_some_and(|context| context.used_tokens >= CACHE_WORTH_TELLING_TOKENS);
        ConversationCache { held: session.prompt_cache.as_ref(), worth_telling: big }
    }

    /// What the open wait `wait` last read of the pull requests it watches, and how its last read went. `None` for one
    /// the singular tool started, which only a hub from before it was dropped can still have open, and which reads
    /// nothing.
    pub fn wait_read(&self, wait: &Wait) -> Option<WaitRead> {
        wait.watch.as_deref().map(protocol::model::PullRequestWatch::read)
    }
}
