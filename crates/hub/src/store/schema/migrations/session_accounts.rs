//! The account each chat is on, since the floor.

/// Whether each chat couldn't start since Automatic found no account with usage to spare, which its record shows, so it
/// moves the revision. No chat from before was stopped so.
pub(super) const NEEDS_AN_ACCOUNT: &str = r"
    ALTER TABLE sessions ADD COLUMN needs_an_account INTEGER NOT NULL DEFAULT 0;
    CREATE TRIGGER sessions_rev_on_needs_an_account
        AFTER UPDATE OF needs_an_account ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";

/// Whether Automatic chose the account each chat is on, which its record shows, so it moves the revision. Every chat
/// from before was put on its account some other way.
pub(super) const PLACED_AUTOMATICALLY: &str = r"
    ALTER TABLE sessions ADD COLUMN placed_automatically INTEGER NOT NULL DEFAULT 0;
    CREATE TRIGGER sessions_rev_on_placed_automatically
        AFTER UPDATE OF placed_automatically ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
