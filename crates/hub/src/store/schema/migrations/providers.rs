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
