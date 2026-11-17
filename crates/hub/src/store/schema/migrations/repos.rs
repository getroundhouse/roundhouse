//! The migrations for what the hub keeps of its repositories. Each has its place in `MIGRATIONS`.

/// The address each repository's checkout fetches from, as a worker on the hub's machine read it from its `origin`,
/// or none, and whether it's to be read, as every repository recorded before this is, and each added after until a
/// worker has said. A repository's record shows its address and not whether it's to be read, so its revision moves
/// with only the columns the record shows, which the trigger names from here on, as the sessions' does.
pub(super) const URLS: &str = r"
    ALTER TABLE repos ADD COLUMN url TEXT;
    ALTER TABLE repos ADD COLUMN url_to_read INTEGER NOT NULL DEFAULT 1;
    DROP TRIGGER repos_rev_on_update;
    CREATE TRIGGER repos_rev_on_update AFTER UPDATE OF name, path, url, created_at_ms ON repos
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE repos SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
