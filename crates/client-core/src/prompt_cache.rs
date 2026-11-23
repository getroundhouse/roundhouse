//! What a view says of the API's cache of a chat's conversation: what it holds while it can expire, which the newest
//! turn's card says, and whether that's worth telling the person of anywhere else, which the tabs, the sidebar, Home
//! and the line above the composer say. Every client says the same of one.

use protocol::model::{PromptCache, Session, SessionStatus};

/// What the API's cache holds of a chat's conversation, as [`crate::records::Snapshot::conversation_cache`] reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversationCache<'a> {
    /// `None` where it holds none of the conversation, as after a compaction or a change of model, or until the agent
    /// has said.
    pub held: Option<&'a PromptCache>,
    /// The chat's context holds at least [`protocol::model::CACHE_WORTH_TELLING_TOKENS`], so writing it again costs
    /// something.
    pub worth_telling: bool,
}

/// What `cache`, `session`'s, holds of the conversation while it can expire: the agent isn't working, which keeps it
/// warm, though it can be waiting on the person. A status this build doesn't know might be working, and claims nothing,
/// as [`SessionStatus`] says, so this says nothing. The newest turn's card says how long it has left from this.
pub fn idle_cache<'a>(session: &Session, cache: ConversationCache<'a>) -> Option<&'a PromptCache> {
    let can_expire = match session.status {
        SessionStatus::Idle
        | SessionStatus::WaitingApproval
        | SessionStatus::Interrupted
        | SessionStatus::Background => true,
        SessionStatus::Running | SessionStatus::Unknown => false,
    };
    cache.held.filter(|_| can_expire)
}

/// The same, when it's worth telling the person anywhere else: the chat is open, and its context is big enough that
/// writing it again costs something. [`PromptCache::state`] says how warm it is at a moment.
pub fn cache_worth_telling<'a>(session: &Session, cache: ConversationCache<'a>) -> Option<&'a PromptCache> {
    idle_cache(session, cache).filter(|_| cache.worth_telling && session.is_open())
}

#[cfg(test)]
mod tests {
    use protocol::model::{
        CACHE_WORTH_TELLING_TOKENS, CacheTtl, ContextUsage, PromptCache, Records, Seq, Session, SessionStatus,
    };
    use serde_json::json;

    use super::{cache_worth_telling, idle_cache};
    use crate::records::HubRecords;

    /// A chat in `status`, as a hub names it, whose agent's context holds `used_tokens`.
    fn session(status: &str, used_tokens: u64) -> Session {
        let context = ContextUsage {
            used_tokens,
            max_tokens: 1_000_000,
            model_tokens: None,
            max_set_by: None,
            parts: Vec::new(),
        };
        Session {
            status: serde_json::from_value::<SessionStatus>(json!(status)).unwrap(),
            last_seq: Seq::new(4),
            context: Some(context),
            prompt_cache: Some(PromptCache { used_at_ms: 0, ttl: CacheTtl::OneHour, model: None, tokens: 0 }),
            ..protocol::fixtures::session("s")
        }
    }

    /// What `session`'s cache is worth, as a client that holds it reads it.
    fn told(session: &Session) -> (bool, bool) {
        let mut held = HubRecords::default();
        held.apply_list(Records { sessions: vec![session.clone()], ..Records::default() });
        let snapshot = held.snapshot();
        let cache = snapshot.conversation_cache(snapshot.session(&session.id).unwrap());
        (idle_cache(session, cache).is_some(), cache_worth_telling(session, cache).is_some())
    }

    #[test]
    fn a_cache_is_worth_telling_of_once_the_context_is_big_enough_and_the_agent_isnt_working() {
        assert_eq!(told(&session("idle", CACHE_WORTH_TELLING_TOKENS)), (true, true));
        assert_eq!(told(&session("waiting_approval", 313_500)), (true, true), "it can expire while it waits");
        assert_eq!(told(&session("idle", CACHE_WORTH_TELLING_TOKENS - 1)), (true, false));
        assert_eq!(told(&session("running", 313_500)), (false, false), "each request keeps it warm");
        let closed = Session { closed_at_ms: Some(1), ..session("idle", 313_500) };
        assert_eq!(told(&closed), (true, false), "nobody will send a closed chat anything");
    }

    #[test]
    fn the_turns_card_says_what_the_cache_holds_whatever_the_size_but_not_while_the_agent_works() {
        assert!(told(&session("idle", 9_000)).0);
        assert!(!told(&session("running", 9_000)).0);
        assert!(!told(&session("thinking_hard", 9_000)).0, "a status from a newer hub might be working");
    }
}
