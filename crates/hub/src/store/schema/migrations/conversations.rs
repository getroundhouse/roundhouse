/// Which device has each chat's conversation while conversation mode is on, as JSON, which its record shows, so it
/// moves the revision. NULL while the mode is off, as it is in every chat from before.
pub(super) const CONVERSATION: &str = r"
    ALTER TABLE sessions ADD COLUMN conversation TEXT;
    CREATE TRIGGER sessions_rev_on_conversation
        AFTER UPDATE OF conversation ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
