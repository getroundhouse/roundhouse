//! What a client shows of GitHub, which it tells the hub by following it.

/// The most issues and pull requests one subscription can show as chips, which is more than a window shows, so a hub
/// refuses a subscription that shows more.
pub const MOST_REFERENCES_SHOWN: usize = 200;
