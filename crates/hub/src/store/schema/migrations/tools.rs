//! The migration that keeps what the worker's tools need across launches, kept apart since it's long. It has its place
//! in `MIGRATIONS`.

/// The port each workspace's worker last served its tools on, and each session's token for them, which every launch of
/// the session's agent is given again, since an agent picks up a call it left waiting only from the same address with
/// the same token. Apart from the records, since no client is shown either, and a change to one is no change to a
/// record.
pub(super) const PORTS_AND_TOKENS: &str = r"
    CREATE TABLE tools_ports (
        workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
        port INTEGER NOT NULL
    );
    CREATE TABLE tools_tokens (
        session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
        token TEXT NOT NULL
    );
";
