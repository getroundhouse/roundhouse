//! Each kind's default account, since the floor.

/// Whether each kind's chats are put on accounts automatically, which the hub's settings show, so it moves their
/// revision as the account chosen does. The account chosen stays, as the one chosen before.
pub(super) const AUTOMATIC: &str = r"
    ALTER TABLE account_defaults ADD COLUMN automatic INTEGER NOT NULL DEFAULT 0;
    DROP TRIGGER account_defaults_rev_on_update;
    CREATE TRIGGER account_defaults_rev_on_update AFTER UPDATE ON account_defaults
        WHEN NEW.account_id != OLD.account_id OR NEW.automatic != OLD.automatic BEGIN
        UPDATE hub_settings SET rev = rev;
    END;
";
