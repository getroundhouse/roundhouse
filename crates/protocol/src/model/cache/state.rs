use super::super::ModelName;
use serde::{Deserialize, Serialize};

/// A chat's cache is about to expire once its TTL divided by this is left, a quarter of it, as [`PromptCache::state`]
/// says, so a 1h cache warns with 15 minutes left.
const WARN_WITH_TTL_DIVIDED_BY: i64 = 4;
/// But never with less left than this, so a 5m cache, a quarter of which is 75 seconds, still leaves time to act.
const WARN_AT_LEAST_MS: i64 = 2 * 60 * 1000;
/// A chat with less in its context than this says nothing of its cache but in its turn's card, since writing that
/// again costs little. About 15k tokens, which every chat shares, stay in the cache anyway, so a chat this big writes
/// about 35k again, which is about $0.28 on Opus 5.5 at list price.
pub const CACHE_WORTH_TELLING_TOKENS: u64 = 50_000;

/// How long an entry in the cache lasts after the last request that read or wrote it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheTtl {
    FiveMinutes,
    OneHour,
    /// One this build doesn't know.
    #[serde(other)]
    Unknown,
}

impl CacheTtl {
    /// How long it is, or `None` for one this build doesn't know.
    pub fn ms(self) -> Option<i64> {
        match self {
            CacheTtl::FiveMinutes => Some(5 * 60 * 1000),
            CacheTtl::OneHour => Some(60 * 60 * 1000),
            CacheTtl::Unknown => None,
        }
    }
}

/// One of the agent's own requests to its model, as the model started to answer it: how the request's input met the
/// cache. The adapter says so then, since by then the input has been read and written. It isn't an event, since a turn
/// makes many. The hub keeps what the newest left on the session's record, as a [`PromptCache`], and needs each in
/// turn, since it finds a cache that had expired by comparing a request with the one before it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CacheRequest {
    /// How long before the adapter said so the request was sent, which is how long the model took to start answering.
    #[serde(default)]
    pub sent_ms_ago: u64,
    /// How long what it wrote lasts, or `None` when it wrote nothing, so what it read lasts as long as it did.
    #[serde(default)]
    pub ttl: Option<CacheTtl>,
    #[serde(default)]
    pub read_tokens: u64,
    #[serde(default)]
    pub written_tokens: u64,
    /// The model it asked, since each model has a cache of its own. `None` when the agent didn't say.
    #[serde(default)]
    pub model: Option<ModelName>,
    /// Whether it's the first request for a prompt, as the adapter knows, rather than the next of the same turn, or
    /// the first of a turn the agent started by itself.
    #[serde(default)]
    pub first_for_prompt: bool,
}

/// What the cache holds of a chat's conversation, as the agent's newest request left it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PromptCache {
    /// When the agent's newest request was sent, by the hub's clock.
    pub used_at_ms: i64,
    pub ttl: CacheTtl,
    /// The model the request asked, whose cache it is. `None` when the agent didn't say.
    #[serde(default)]
    pub model: Option<ModelName>,
    /// How much of the conversation it holds: what the request read from it and wrote to it. 0 from a hub that
    /// didn't say.
    #[serde(default)]
    pub tokens: u64,
}

/// How warm a chat's cache is at some moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheState {
    /// It's warm, with this long left.
    Warm { left_ms: i64 },
    /// It's warm, and about to expire, with this long left.
    Expiring { left_ms: i64 },
    /// It expired this long ago.
    Expired { ago_ms: i64 },
}

impl PromptCache {
    /// When it expires, by the hub's clock, or `None` for a TTL this build doesn't know.
    pub fn expires_at_ms(&self) -> Option<i64> {
        self.ttl.ms().map(|ttl| self.used_at_ms.saturating_add(ttl))
    }

    /// When it's about to expire, by the hub's clock: with a quarter of its TTL left, or 2 minutes, whichever is
    /// longer. `None` for a TTL this build doesn't know.
    pub fn warns_at_ms(&self) -> Option<i64> {
        let ttl = self.ttl.ms()?;
        let warn_ms = (ttl / WARN_WITH_TTL_DIVIDED_BY).max(WARN_AT_LEAST_MS);
        Some(self.used_at_ms.saturating_add(ttl).saturating_sub(warn_ms))
    }

    /// How warm it is at `now_ms`, by the hub's clock, as [`PromptCache::warns_at_ms`] says. `None` for a TTL this
    /// build doesn't know.
    pub fn state(&self, now_ms: i64) -> Option<CacheState> {
        let (expires_at_ms, warns_at_ms) = (self.expires_at_ms()?, self.warns_at_ms()?);
        // Never more than the TTL, for a hub whose clock runs ahead of this one's.
        let left_ms = expires_at_ms.saturating_sub(now_ms).min(self.ttl.ms()?);
        Some(if left_ms <= 0 {
            CacheState::Expired { ago_ms: left_ms.saturating_neg() }
        } else if now_ms >= warns_at_ms {
            CacheState::Expiring { left_ms }
        } else {
            CacheState::Warm { left_ms }
        })
    }

    /// The share of its TTL it has left at `now_ms`, from 1 as it's written or read to 0 once it has expired. `None` for
    /// a TTL this build doesn't know.
    pub fn share_left(&self, now_ms: i64) -> Option<f32> {
        let ttl = self.ttl.ms()?;
        let left = self.used_at_ms.saturating_add(ttl).saturating_sub(now_ms).clamp(0, ttl);
        Some(share(left, ttl))
    }
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a share of an hour in milliseconds, drawn as a wedge a few pixels across, which an f32 keeps"
)]
fn share(part: i64, of: i64) -> f32 {
    (part as f64 / of as f64) as f32
}
