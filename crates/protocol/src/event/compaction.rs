//! What became of a compaction, and who started it.

use serde::{Deserialize, Serialize};

/// How a compaction went. One this build doesn't know reads as `Unknown`, and so does one recorded before this was
/// kept.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CompactionOutcome {
    /// The agent carries on from `summary` in place of what it held.
    Compacted {
        #[serde(default)]
        by: CompactedBy,
        /// How much the context held before, and after, in tokens.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        took_ms: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    /// It couldn't, and why, when the agent said. The agent doesn't say who started it.
    Failed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    #[default]
    #[serde(other)]
    Unknown,
}

/// Who started a compaction. One this build doesn't know reads as `Unknown`, and so does one the agent didn't say.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactedBy {
    /// Someone asked, such as with `/compact`.
    Asked,
    /// The agent, as its context filled.
    Agent,
    #[default]
    #[serde(other)]
    Unknown,
}
