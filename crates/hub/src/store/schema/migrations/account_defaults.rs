//! Each kind's explicit default survives renewal, expiry and disconnect.

pub(super) const ACCOUNT_DEFAULTS: &str = r"
    CREATE TABLE account_defaults (
        agent TEXT PRIMARY KEY,
        account_id TEXT NOT NULL REFERENCES accounts(id)
    );
    CREATE TRIGGER account_defaults_rev_on_insert AFTER INSERT ON account_defaults BEGIN
        UPDATE hub_settings SET rev = rev;
    END;
    CREATE TRIGGER account_defaults_rev_on_update AFTER UPDATE ON account_defaults
        WHEN NEW.account_id != OLD.account_id BEGIN
        UPDATE hub_settings SET rev = rev;
    END;
    INSERT INTO account_defaults (agent, account_id)
        SELECT login_agent, id FROM (
            SELECT login_agent, id, row_number() OVER (
                PARTITION BY login_agent ORDER BY
                    (login_ends_at_ms IS NULL OR login_ends_at_ms > unixepoch('subsec') * 1000) DESC,
                    login_connected_at_ms DESC, id
            ) AS preference
            FROM accounts WHERE login_agent IS NOT NULL AND login_file IS NOT NULL
                AND login_connected_at_ms IS NOT NULL
        ) WHERE preference = 1;
";
