//! The projects the hub has, since the floor.

/// Whether a push from each checkout went round the git credential workers are given, which nothing has read or written
/// since the hub left GitHub to the machine on the local provider. A hub that did can't open a database this has run
/// on, since a hub refuses one from a newer build, so nothing is carried.
pub(super) const PUSH_AROUND_DROPPED: &str = r"
    ALTER TABLE repos DROP COLUMN push_around;
";
