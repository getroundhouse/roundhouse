//! The migration that keeps each model's own week of an account's limits apart, kept apart since it's long. It has its
//! place in `MIGRATIONS`.

/// Each of an account's limits is kept by its window and, for a model's own week, by the model, as its provider labels
/// it. Any other window's model is empty rather than none, since SQLite counts no two NULLs in a key as equal, so
/// hearing that limit again would add a row rather than replace it. SQLite can't change a table's key, so the table is
/// made again with what it held before its triggers are, and nothing it held is a change to an account's record. A
/// model's week can be forgotten now, once an answer that lists every one leaves it out, and that's a change to the
/// record too.
pub(super) const BY_MODEL: &str = r"
    CREATE TABLE account_limits_by_model (
        account_id TEXT NOT NULL,
        window TEXT NOT NULL,
        model TEXT NOT NULL DEFAULT '',
        used_percent INTEGER NOT NULL,
        resets_at_ms INTEGER,
        heard_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, window, model)
    );
    INSERT INTO account_limits_by_model (account_id, window, used_percent, resets_at_ms, heard_at_ms)
        SELECT account_id, window, used_percent, resets_at_ms, heard_at_ms FROM account_limits;
    DROP TABLE account_limits;
    ALTER TABLE account_limits_by_model RENAME TO account_limits;
    CREATE TRIGGER account_limits_rev_on_insert AFTER INSERT ON account_limits BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.account_id;
    END;
    CREATE TRIGGER account_limits_rev_on_update AFTER UPDATE OF used_percent, resets_at_ms ON account_limits
        WHEN NEW.used_percent IS NOT OLD.used_percent OR NEW.resets_at_ms IS NOT OLD.resets_at_ms BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.account_id;
    END;
    CREATE TRIGGER account_limits_rev_on_delete AFTER DELETE ON account_limits BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = OLD.account_id;
    END;
";
