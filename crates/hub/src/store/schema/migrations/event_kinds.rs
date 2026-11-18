//! The index that reads a session's events of a kind.

/// Each session's events of each kind, in the order they were recorded, so a read of one kind, such as a chat's
/// prompts or its newest failure, takes only those events, already in order. Without `seq`, SQLite took such a read's
/// order from the primary key instead, and read every event of the session to find the few it wanted: about 24,000
/// pages of the database to list the prompts of a chat of 37,000 events, against under a hundred this way.
pub(super) const IN_ORDER: &str = r"
    DROP INDEX events_by_kind;
    CREATE INDEX events_by_kind ON events (session_id, kind, seq);
";
