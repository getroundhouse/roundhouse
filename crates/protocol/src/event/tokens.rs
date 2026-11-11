//! What a turn asked of its models, in tokens.

use serde::{Deserialize, Serialize};

/// What was asked of the models and what they wrote, in tokens, summed over the requests made. A request's input is in
/// one of three parts, by whether the cache had it: read fresh, read from the cache, or written to the cache for a
/// later request to read.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    /// Their thinking included.
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}
