//! What the API's prompt cache holds of a chat's conversation, and until when.
//!
//! The API keeps what a request reads and writes for a while, so the next request needn't send it again, and a request
//! after that has expired writes the whole conversation again, which is the most a prompt can cost. An entry lasts its
//! TTL from the start of the last request that read or wrote it, so a chat's cache expires one TTL after its agent's
//! newest request was sent. A subagent caches a conversation of its own, so only the agent's own requests count.

#[cfg(test)]
mod tests;

mod state;

pub use state::{CACHE_WORTH_TELLING_TOKENS, CacheRequest, CacheState, CacheTtl, PromptCache};
