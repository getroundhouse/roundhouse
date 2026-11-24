//! What the hub observes of a chat that the chat's record doesn't hold.

use serde::{Deserialize, Serialize};

use crate::model::{CACHE_WORTH_TELLING_TOKENS, ContextUsage, PromptCache};

/// What the API's cache holds of a chat's conversation, as the hub last worked it out from its agent's requests, and
/// whether the chat's context is big enough that writing it again costs something, which is all a sidebar needs of the
/// context.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConversationCache {
    /// `None` where the cache holds none of the conversation, as after a compaction or a change of model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<PromptCache>,
    /// The chat's context holds at least [`CACHE_WORTH_TELLING_TOKENS`], as its agent last said, so writing it again
    /// costs something. `false` while the context isn't known.
    #[serde(default)]
    pub worth_telling: bool,
}

impl ConversationCache {
    /// What the cache holds, `held`, beside how full the context is, `context`, which isn't known where it's `None`.
    pub fn worked_out(held: Option<PromptCache>, context: Option<&ContextUsage>) -> Self {
        Self { held, worth_telling: context.is_some_and(|context| context.used_tokens >= CACHE_WORTH_TELLING_TOKENS) }
    }
}
