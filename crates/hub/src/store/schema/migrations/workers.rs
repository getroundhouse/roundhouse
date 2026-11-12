//! The migrations for what the hub keeps of its workers. Each has its place in `MIGRATIONS`.

/// Which run's worker each session last recorded something from that the worker numbered, and its number, so a hub
/// that connects to the worker again drops what the worker sends it again. Not on the record, so not in its trigger.
pub(super) const RECORDED_FROM_WORKER: &str = r"
    ALTER TABLE sessions ADD COLUMN recorded_run INTEGER;
    ALTER TABLE sessions ADD COLUMN recorded_through INTEGER;
";
