//! The account each chat is on.

/// The account a session is on, which its record shows, so it moves the revision. A session from before is on none, as
/// a NULL says, so its next launch under a login the hub holds puts it on its kind's default then, as the default it
/// followed until now would have.
pub(super) const ASSIGNED_ACCOUNTS: &str = r"
    ALTER TABLE sessions ADD COLUMN assigned_account_id TEXT REFERENCES accounts(id);
    CREATE TRIGGER sessions_rev_on_assigned_account
        AFTER UPDATE OF assigned_account_id ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
