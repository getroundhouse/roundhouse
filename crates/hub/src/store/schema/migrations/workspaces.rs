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
