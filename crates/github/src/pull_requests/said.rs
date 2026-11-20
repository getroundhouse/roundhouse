//! Reading what GitHub's GraphQL answered, and the errors a read of pull requests fails with, by what each means for
//! whoever asked: [`ErrorCode::Conflict`] for pull requests that moved while they were read, which reading again can
//! get past, [`ErrorCode::Unavailable`] and [`ErrorCode::Unsupported`] for what reading again won't change, and
//! [`ErrorCode::Worker`] for what may pass, such as GitHub failing or limiting the hub for now.

use protocol::client::{ApiError, ErrorCode};
use serde_json::Value;

/// GitHub refusing the token a read went as. Reading again may get past it, once `gh` has logged in again, or the
/// default has another token, so waits go on waiting.
pub(super) fn token_refused() -> ApiError {
    ApiError::new(ErrorCode::Worker, "GitHub refused the token the pull requests are read with")
}

/// What GitHub left out or wrote otherwise than this reads, so nothing of what it answered is taken.
pub(crate) fn incomplete() -> ApiError {
    ApiError::new(ErrorCode::Worker, "GitHub's answer was incomplete, so nothing of it was taken")
}

/// Pull requests that moved while they were read, as when a branch is pushed or the branch they go into moves.
pub(super) fn changed() -> ApiError {
    ApiError::new(
        ErrorCode::Conflict,
        "the pull requests kept changing while they were read, as when a branch is pushed or the branch they go into \
         moves; read them again shortly",
    )
}

pub(crate) fn string(value: &Value, key: &str) -> Result<String, ApiError> {
    value[key].as_str().map(str::to_owned).ok_or_else(incomplete)
}

pub(crate) fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, ApiError> {
    value[key].as_array().ok_or_else(incomplete)
}

/// The cursor of a connection's next page, or `None` on its last.
pub(super) fn next(connection: &Value) -> Result<Option<String>, ApiError> {
    match connection["pageInfo"]["hasNextPage"].as_bool() {
        Some(false) => Ok(None),
        Some(true) => string(&connection["pageInfo"], "endCursor").map(Some),
        None => Err(incomplete()),
    }
}

/// GitHub's hourly limit on GraphQL for a token, as GitHub counted it in an answer. It counts points, not calls: a
/// query costs one, or more when it asks for lists within lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimit {
    pub limit: u32,
    pub remaining: u32,
    /// When it's back to `limit`.
    pub resets_at_ms: i64,
}

/// What's left of GitHub's limit after a query, from its `rateLimit`.
pub(super) fn usage(answer: &Value) -> Option<RateLimit> {
    let usage = answer.pointer("/data/rateLimit").filter(|usage| usage.is_object())?;
    let count = |key: &str| usage[key].as_u64().and_then(|n| u32::try_from(n).ok());
    let resets_at = usage["resetAt"].as_str()?.parse::<jiff::Timestamp>().ok()?;
    Some(RateLimit { limit: count("limit")?, remaining: count("remaining")?, resets_at_ms: resets_at.as_millisecond() })
}

/// What an error GitHub gave means, by its `type` and what it says: GitHub's schema lacking what's asked, as on a
/// GitHub Enterprise Server from before stacks, is unsupported, being refused access or nothing being there is
/// unavailable, and anything else, such as GitHub limiting the hub, may pass.
pub(crate) fn failed(error: &Value) -> ApiError {
    let kind = error["type"].as_str().unwrap_or_default();
    let message = error["message"].as_str().unwrap_or("GitHub said nothing more");
    let unsupported = kind == "undefinedField" || message.contains("doesn't exist on type");
    let code = match kind {
        _ if unsupported => ErrorCode::Unsupported,
        "NOT_FOUND" | "FORBIDDEN" | "UNAUTHORIZED" => ErrorCode::Unavailable,
        _ => ErrorCode::Worker,
    };
    ApiError::new(code, format!("GitHub couldn't be read: {message}"))
}
