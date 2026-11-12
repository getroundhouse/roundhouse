//! Keeping what the API's cache holds of the conversation, from what the agent's adapter says of each of its requests.

use protocol::event::EventBody;
use protocol::model::{CacheRequest, PromptCache, RunId};
use tracing::error;

use super::{LiveSession, Origin};

/// The longest a request is taken to have waited for its model to start answering, so a wrong number can't put when it
/// was sent far back, and with it how long the chat was idle before the next.
const LONGEST_WAIT_MS: u64 = 10 * 60 * 1000;

impl LiveSession {
    /// One of the agent's own requests was sent `request.sent_ms_ago` before `now_ms`, by the hub's clock. Only the
    /// current run speaks for the session. If the request found the cache expired, that's recorded, as
    /// [`found_expired`] says.
    pub(super) async fn note_cache(&self, run: RunId, request: CacheRequest, now_ms: i64) {
        let previous = {
            let state = self.state.lock();
            if state.agent.is_none_or(|agent| agent.run != run) {
                return;
            }
            state.prompt_cache.clone()
        };
        let waited_ms = i64::try_from(request.sent_ms_ago.min(LONGEST_WAIT_MS)).unwrap_or(0);
        let sent_at_ms = now_ms.saturating_sub(waited_ms);
        if let Some(expired) = found_expired(previous.as_ref(), &request, sent_at_ms) {
            self.record_or_log(Origin::Hub, expired).await;
        }
        let Some(cache) = after(previous.as_ref(), &request, sent_at_ms) else { return };
        self.announce_cache(Some(cache)).await;
    }

    /// Keeps `cache`, as [`LiveSession::keep_cache`] does, and announces it unless the agent is working, since it makes
    /// a request every few seconds as it works, and the end of the turn announces what the last one left.
    pub(super) async fn announce_cache(&self, cache: Option<PromptCache>) {
        if !self.keep_cache(cache).await || self.status().is_working() {
            return;
        }
        let Ok(hub) = self.hub() else { return };
        if let Err(e) = self.announce(&hub).await {
            error!(session = %self.id, "could not announce what the cache holds of the conversation: {e}");
        }
    }

    /// The cache holds `cache` of the conversation from here on, or none of it. It isn't announced, so what changed it
    /// can announce it with the rest. False if it couldn't be kept.
    pub(super) async fn keep_cache(&self, cache: Option<PromptCache>) -> bool {
        let Ok(hub) = self.hub() else { return false };
        let (id, kept) = (self.id.clone(), cache.clone());
        if let Err(e) = hub.blocking(move |store| store.set_session_cache(&id, kept.as_ref())).await {
            error!(session = %self.id, "could not record what the cache holds of the conversation: {e}");
            return false;
        }
        self.state.lock().prompt_cache = cache;
        true
    }
}

/// What the cache holds once `request` has been sent at `sent_at_ms`, after `previous`. A request that wrote nothing
/// leaves what it read lasting as long as it did, so with nothing before it, nothing can be said.
fn after(previous: Option<&PromptCache>, request: &CacheRequest, sent_at_ms: i64) -> Option<PromptCache> {
    let ttl = request.ttl.or(previous.map(|cache| cache.ttl))?;
    let model = request.model.clone().or_else(|| previous.and_then(|cache| cache.model.clone()));
    let tokens = request.read_tokens.saturating_add(request.written_tokens);
    Some(PromptCache { used_at_ms: sent_at_ms, ttl, model, tokens })
}

/// What `request` found, if it found the cache expired: it's the first for a prompt, it came after another to the same
/// model, it read less than half of what that one left in the cache, and it wrote more than it read. A cache that
/// holds the conversation is read at least as far as the request before left it, however much the prompt adds, so one
/// that has expired is read only for what every chat shares, such as the tools, and written nearly whole. Another
/// model's cache doesn't hold the conversation either, and the person chose that, so it isn't this. *Unconfirmed:* a
/// change to the agent's tools, or to how it thinks, would leave the cache unread too, and be taken for this.
fn found_expired(previous: Option<&PromptCache>, request: &CacheRequest, sent_at_ms: i64) -> Option<EventBody> {
    let previous = previous.filter(|_| request.first_for_prompt)?;
    let same_model = match (&previous.model, &request.model) {
        (Some(before), Some(now)) => before == now,
        _ => true,
    };
    let unread = request.read_tokens < previous.tokens / 2 && request.written_tokens > request.read_tokens;
    (same_model && unread).then(|| EventBody::CacheExpired {
        idle_ms: u64::try_from(sent_at_ms.saturating_sub(previous.used_at_ms)).unwrap_or(0),
        read_tokens: request.read_tokens,
        written_tokens: request.written_tokens,
    })
}

#[cfg(test)]
mod tests;
