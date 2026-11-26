//! What the session tells clients of what the API's cache holds of its conversation, as the observation
//! `SessionCache`, worked out from what the session last heard of its agent's requests and from how full its context
//! is, the observation `SessionContext`.

use protocol::model::{ConversationCache, PromptCache, Session};

use super::super::LiveSession;
use crate::hub::{Hub, SessionCache, SessionContext, SessionKey};

/// What the API's cache holds of the conversation, as the session last heard, which the next request is compared with,
/// as `found_expired` says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum CacheHeld {
    /// Nothing has said since the hub last forgot it, as one that first starts on a database from before it kept it
    /// as an observation, or whose cache file was emptied, does: so the next request can't be told to have found it
    /// expired, and nothing is told of it until something says.
    #[default]
    NotKnown,
    /// It holds this, or none of the conversation.
    Known(Option<PromptCache>),
}

impl CacheHeld {
    /// What it holds, if anything is known of it.
    pub(crate) fn cache(&self) -> Option<&PromptCache> {
        match self {
            CacheHeld::Known(held) => held.as_ref(),
            CacheHeld::NotKnown => None,
        }
    }

    /// As the hub last told it of `session`, as the observation of it says, if it's known.
    pub(crate) fn told_of(hub: &Hub, session: &Session) -> Self {
        let of = SessionKey { workspace_id: session.workspace_id.clone(), session_id: session.id.clone() };
        hub.observation_value::<SessionCache>(&of).map_or(CacheHeld::NotKnown, |told| CacheHeld::Known(told.held))
    }
}

impl LiveSession {
    /// The cache holds `cache` of the conversation from here on, or none of it. It's told of unless the agent is
    /// working, since it makes a request every few seconds as it works, and the end of the turn tells of what the last
    /// one left, as the session's record was announced when the cache was a field of it.
    pub(in super::super) async fn cache_holds(&self, cache: Option<PromptCache>) {
        let _in_order = self.recording.lock().await;
        self.state.lock().prompt_cache = CacheHeld::Known(cache);
        self.tell_cache_unless_working().await;
    }

    /// Tells clients what the cache holds, as [`LiveSession::tell_cache`] does, unless the agent is working, so it's
    /// told as the agent stops, as a turn ends. The caller holds the lock events are recorded under.
    pub(in super::super) async fn tell_cache_unless_working(&self) {
        if !self.status().is_working() {
            self.tell_cache().await;
        }
    }

    /// The cache holds none of the conversation from here on, and that's told of whatever the agent is doing, since
    /// what emptied it, such as another model or account, was the person's, and changes the session's record too.
    pub(in super::super) async fn forget_cache(&self) {
        let _in_order = self.recording.lock().await;
        self.cache_forgotten().await;
    }

    /// The same, under the lock events are recorded under, which the caller holds.
    pub(in super::super) async fn cache_forgotten(&self) {
        self.state.lock().prompt_cache = CacheHeld::Known(None);
        self.tell_cache().await;
    }

    /// Tells clients what the cache holds, as the session last heard, and whether the context is big enough to tell
    /// of, as its observation says now. Nothing, while nothing is known of what the cache holds. The caller holds the
    /// lock events are recorded under, from reading both until what it works out is kept, so what's told last is worked
    /// out from what's newest of both, since the session is the only one to tell of its cache.
    pub(in super::super) async fn tell_cache(&self) {
        let Ok(hub) = self.hub() else { return };
        let CacheHeld::Known(held) = self.state.lock().prompt_cache.clone() else { return };
        let as_of_ms = held.as_ref().map_or_else(crate::store::now_ms, |held| held.used_at_ms);
        let context = hub.observation_value::<SessionContext>(&self.observed_as());
        self.told_cache(&hub, ConversationCache::worked_out(held, context.as_ref()), as_of_ms).await;
    }

    /// Tells clients whether the context is big enough to tell of, now that how full it is has changed, beside what the
    /// cache was last told to hold. Nothing, while nothing is known of what it holds.
    pub(in super::super) async fn tell_cache_worth_telling(&self) {
        let Ok(hub) = self.hub() else { return };
        let _in_order = self.recording.lock().await;
        let of = self.observed_as();
        let Some(told) = hub.observation_value::<SessionCache>(&of) else { return };
        let context = hub.observation_value::<SessionContext>(&of);
        let cache = ConversationCache::worked_out(told.held.clone(), context.as_ref());
        if cache != told {
            self.told_cache(&hub, cache, crate::store::now_ms()).await;
        }
    }

    /// Keeps that the cache is as `cache` says, as of `as_of_ms`, which tells clients of a change.
    async fn told_cache(&self, hub: &Hub, cache: ConversationCache, as_of_ms: i64) {
        hub.observation_worked_out::<SessionCache>(self.observed_as(), cache, as_of_ms).await;
    }
}
