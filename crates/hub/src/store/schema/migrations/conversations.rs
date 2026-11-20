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

/// How the person was talking with the agent as each event was recorded, as the event says: NULL for typing, and
/// `conversation` in conversation mode. NULL in every event from before, as most were typed, and the mode's ends weren't
/// recorded to stamp the rest by.
pub(super) const INTERACTION: &str = r"
    ALTER TABLE events ADD COLUMN interaction TEXT;
";

/// How each reply said out loud ended for the person hearing it, by the reply's message, which every device draws the
/// reply from, so it moves the revision.
pub(super) const REPLIES_HEARD: &str = r"
    CREATE TABLE replies_heard (
        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        message TEXT NOT NULL,
        ending TEXT NOT NULL,
        rev INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (session_id, message)
    );
    CREATE TRIGGER replies_heard_rev_on_insert AFTER INSERT ON replies_heard BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE replies_heard SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER replies_heard_rev_on_update AFTER UPDATE ON replies_heard WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE replies_heard SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER replies_heard_rev_on_delete AFTER DELETE ON replies_heard BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
";
