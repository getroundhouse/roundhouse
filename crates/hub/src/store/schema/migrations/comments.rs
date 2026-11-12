//! The migration for comments on each workspace's diff, kept apart from the others to keep `MIGRATIONS` short. It has
//! its place in `MIGRATIONS`.

/// Comments on each workspace's diff, each on a line of a file on one side of it.
pub(super) const DIFF_COMMENTS: &str = r"
    CREATE TABLE diff_comments (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        path TEXT NOT NULL,
        side TEXT NOT NULL,
        line INTEGER NOT NULL,
        text TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
    CREATE INDEX diff_comments_by_workspace ON diff_comments (workspace_id, created_at_ms);
";
