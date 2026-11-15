//! The account each chat is on, and where its login comes from.

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

/// Where each session's login comes from, in place of the account it was on: the hub's, for the account it was on,
/// which its record names from here, since it's the one the session is on, and none for the rest. So a session on none,
/// such as one whose agent ran under its machine's own login, is still on none, and its next launch takes its kind's
/// default, as it would have.
pub(super) const ACCOUNT_SOURCES: &str = r"
    ALTER TABLE sessions ADD COLUMN account_source TEXT;
    UPDATE sessions SET account_source = 'hub' WHERE assigned_account_id IS NOT NULL;
    UPDATE sessions SET account_id = assigned_account_id
        WHERE assigned_account_id IS NOT NULL AND account_id IS NOT assigned_account_id;
    DROP TRIGGER sessions_rev_on_assigned_account;
    ALTER TABLE sessions DROP COLUMN assigned_account_id;
    CREATE TRIGGER sessions_rev_on_account_source
        AFTER UPDATE OF account_source ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
