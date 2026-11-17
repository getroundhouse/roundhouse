//! The CLI trying a failed request to its model again, which is live output, as the session's tail holds it.

use protocol::tail::Retrying;
use serde_json::Value;

use super::{Parsed, other};

/// `api_retry`, which the CLI sends before each try of a request that failed: which try comes next, of how many, the
/// HTTP status the last failed with, when it got one, and how long it waits first. One with no try is kept for a later
/// build.
pub(super) fn read(value: &Value) -> Parsed {
    let Some(attempt) = value.get("attempt").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()) else {
        return other("system/api_retry");
    };
    let number = |key: &str| value.get(key).and_then(Value::as_u64);
    Parsed::Retrying(Retrying {
        run: None,
        attempt,
        of: number("max_retries").and_then(|n| u32::try_from(n).ok()),
        status: number("error_status").and_then(|n| u16::try_from(n).ok()),
        delay_ms: number("retry_delay_ms"),
    })
}
