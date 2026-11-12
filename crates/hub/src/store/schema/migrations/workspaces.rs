//! The migrations for what a workspace's record keeps of its worktree's git and of GitHub, kept apart from the others to
//! keep `MIGRATIONS` short. Each has its place in `MIGRATIONS`.

/// What the hub last found of each workspace's pull request, as JSON, which is `null` when it found none. NULL until it
/// has looked, as for every workspace from before this. The workspaces' trigger takes every column.
pub(super) const PULL_REQUEST: &str = r"
    ALTER TABLE workspaces ADD COLUMN pull_request TEXT;
";

/// What each workspace's worker last read of its worktree's branches, as JSON, and the branch its pull request was last
/// looked for on. NULL until a worker watches it, and until it's next looked for, as for every workspace from before
/// this. The workspaces' trigger takes every column.
pub(super) const BRANCHES: &str = r"
    ALTER TABLE workspaces ADD COLUMN branches TEXT;
    ALTER TABLE workspaces ADD COLUMN pull_request_branch TEXT;
";

/// Which build each workspace's worker is, while the hub carries on with one of another build than its own, as after an
/// update. NULL otherwise, as for every workspace from before this. The workspaces' trigger takes every column.
pub(super) const WORKER_BUILD: &str = r"
    ALTER TABLE workspaces ADD COLUMN worker_build TEXT;
";

/// Each of a workspace's other worktrees, made for its work, with a revision like every record's. `state` is
/// `creating`, `ready` or `removed`, and no two that aren't removed share a path. `ask_key` is the worker's key for the
/// ask that made it, unique within the session that asked, so asking again makes it once. `made_by` is JSON, and so
/// is `branches`, NULL until a worker has watched them.
pub(super) const CHILD_WORKTREES: &str = r"
    CREATE TABLE child_worktrees (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        path TEXT NOT NULL,
        branch TEXT NOT NULL,
        session_id TEXT NOT NULL REFERENCES sessions(id),
        made_by TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        state TEXT NOT NULL,
        removed_at_ms INTEGER,
        branches TEXT,
        ask_key TEXT,
        rev INTEGER NOT NULL DEFAULT 0,
        UNIQUE (session_id, ask_key)
    );
    CREATE UNIQUE INDEX child_worktrees_there ON child_worktrees (path) WHERE state != 'removed';
    CREATE INDEX child_worktrees_of ON child_worktrees (workspace_id);
    CREATE TRIGGER child_worktrees_rev_on_insert AFTER INSERT ON child_worktrees BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE child_worktrees SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER child_worktrees_rev_on_update AFTER UPDATE ON child_worktrees WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE child_worktrees SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER child_worktrees_rev_on_delete AFTER DELETE ON child_worktrees BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
";
