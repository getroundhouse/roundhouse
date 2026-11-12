//! The migrations for running workspaces on providers, kept apart from the others to keep `MIGRATIONS` short. Each has
//! its place in `MIGRATIONS`.

/// Who this hub is, which it tells each provider's adapter, so an adapter works only with this hub's instances. Made
/// once, here, and kept with the workspaces whose instances it names, so a copy restored from a backup is the same hub.
pub(super) const HUB_IDENTITY: &str = r"
    CREATE TABLE hub_identity (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        hub_id TEXT NOT NULL
    );
    INSERT INTO hub_identity (id, hub_id) VALUES (1, lower(hex(randomblob(16))));
";

/// Each instance the hub asks a provider for, recorded before it asks, as `store::instances` says. Each workspace's is
/// recorded as made, or as being made if the workspace is.
pub(super) const INSTANCES: &str = r"
    CREATE TABLE instances (
        id TEXT PRIMARY KEY,
        provider TEXT NOT NULL,
        stage TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
    INSERT INTO instances (id, provider, stage, created_at_ms)
        SELECT instance_id, provider, CASE status WHEN 'creating' THEN 'creating' ELSE 'ready' END, created_at_ms
            FROM workspaces;
";
