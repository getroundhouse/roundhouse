//! The migration that keeps the hub's own settings, kept apart to keep `MIGRATIONS` short. It has its place there.

/// The hub's own settings, one row of them, which is a record like the rest. It keeps its machine awake unless
/// someone has chosen otherwise. The row goes in after its trigger, so it has a revision from the start.
pub(super) const HUB_SETTINGS: &str = r"
    CREATE TABLE hub_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        keep_awake INTEGER NOT NULL DEFAULT 1,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER hub_settings_rev_on_insert AFTER INSERT ON hub_settings BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE hub_settings SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
    CREATE TRIGGER hub_settings_rev_on_update AFTER UPDATE ON hub_settings WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE hub_settings SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
    INSERT INTO hub_settings (id) VALUES (1);
";
