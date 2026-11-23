use serde::{Deserialize, Serialize};

/// How fresh an observation is, and what it says. So no client shows an old reading as current, or takes "not known"
/// for "none". A client that holds no copy of an observation reads it as `NotKnown` too.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "freshness", rename_all = "snake_case")]
pub enum Observed<T> {
    /// Never read, or emptied since.
    NotKnown,
    /// Kept up to date now, as what a client follows is.
    Current { value: T },
    /// As it was last read, at `as_of_ms`, by the hub's clock, in milliseconds since the Unix epoch, since nothing
    /// keeps it up to date now.
    AsOf { value: T, as_of_ms: i64 },
    /// A freshness a newer hub says, which this build can't read, so it holds nothing a view can show. A view draws
    /// it as it draws `NotKnown`.
    #[serde(other)]
    Unknown,
}
