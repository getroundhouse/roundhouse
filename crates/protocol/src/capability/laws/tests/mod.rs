//! The laws, each checked against logs that keep it and logs that break it.

/// Logs to check, a made-up event at a time, and what they break.
mod log;

/// Each thing a log settles is settled once, after it was made, and nothing of a run comes after its exit.
mod order;

/// What a run records needs only what it claims.
mod claims;
