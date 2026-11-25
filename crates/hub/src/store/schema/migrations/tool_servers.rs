//! The migration for the tool servers the hub has of its own, which has its place in `MIGRATIONS`.

/// Each tool server, with a `rev` like every record's. `name` is the person's, and which two can't both be had is the
/// hub's to check, as `protocol` says, so nothing here makes one unique. `auth` is how it knows the person, as JSON,
/// and `secret_file` the name of the file that holds what it knows them by, NULL for one that doesn't know them.
pub(super) const TOOL_SERVERS: &str = r"
    CREATE TABLE tool_servers (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        url TEXT NOT NULL,
        auth TEXT NOT NULL,
        secret_file TEXT,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER tool_servers_rev_on_insert AFTER INSERT ON tool_servers BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE tool_servers SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER tool_servers_rev_on_update AFTER UPDATE ON tool_servers WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE tool_servers SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER tool_servers_rev_on_delete AFTER DELETE ON tool_servers BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
";
