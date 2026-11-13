//! What the model's context holds, how it's made room in, and what the API's cache keeps of it.

use serde::{Deserialize, Serialize};

use super::Claimed;
use super::area::area;
use crate::model::CacheTtl;

area! { "the context";
    /// How full the context is, in tokens, and the most it holds, as of the agent's newest request, after each one or
    /// when the adapter asks, and what set the most where something other than the model did, as a
    /// [`Report::Context`](super::Report::Context). Only the newest matters, which the hub keeps on the session's
    /// record.
    Usage => usage: Claimed,
    /// The context's parts, such as the system prompt, the tools and the messages, each with its tokens and whether
    /// it's in the context, loaded when needed, or room kept back.
    UsageParts => usage_parts: Claimed,
    /// The agent compacts by itself as its context fills, and reports it as it starts and as it ends, whether it
    /// carries on with the turn or not.
    AutoCompaction => auto_compaction: Claimed,
    /// The agent compacts when asked, between turns, as a turn of its own or not, and names the request on the
    /// compaction, with what it was asked to keep where [`Compaction::keeping`] says it reads that. The hub sends
    /// nothing to keep to one that doesn't.
    Compaction => compaction: Compaction,
    /// A compaction reports how much of its summary it has written as it goes, since it can take minutes.
    CompactionProgress => compaction_progress: Claimed,
    /// A compaction reports the summary it wrote, which is what the agent now has of the conversation before it.
    CompactionSummary => compaction_summary: Claimed,
    /// A compaction reports how many tokens the context held before it, and after.
    CompactionTokens => compaction_tokens: Claimed,
    /// Old calls' results are dropped from the context to make room without compacting, and each call whose result
    /// went is reported, which the hub records, so a client can say the agent no longer sees it.
    Pruning => pruning: Claimed,
    /// Each of the agent's own requests to its model says what the API's cache read and wrote for it, as
    /// [`Cache::kind`] says, as a [`Report::Cache`](super::Report::Cache), so the hub can tell when a prompt found the
    /// cache expired. The hub keeps what the newest left on the session's record.
    Cache => cache: Cache,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Compaction {
    pub keeping: Keeping,
}

/// Whether the agent reads what a compaction is asked to keep, which a client says before the person types it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Keeping {
    /// It does, as Claude's `/compact` does with what follows it.
    Read,
    /// It doesn't, as Codex and opencode don't, so a client doesn't ask.
    Ignored,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Cache {
    pub kind: CacheKind,
}

/// How the API caches the conversation between requests.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CacheKind {
    /// The agent marks what to cache and for how long, as Claude does, and each request says which of these it wrote
    /// for, so the hub knows when the cache expires.
    Marked { ttls: Vec<CacheTtl> },
    /// The API caches by itself, for as long as it chooses, as Codex's models' API does, so a request says what it read
    /// and wrote, and the hub can't say when the cache expires.
    Automatic,
    #[serde(other)]
    Unknown,
}
