//! The migration that keeps the GitHub accounts the hub holds a login for, kept apart since it's long. It has its place
//! in `MIGRATIONS`.

/// Each GitHub account the hub holds, or held, a login for, as a record the feed announces: who it is on GitHub, by the
/// user's number and the name GitHub last gave, and the login while the hub holds one. `login_kind` is how it was
/// connected, as `GithubLoginKind` names it, and `login_file` names the file in the GitHub logins' directory its token
/// is in, as `crate::login_files` says, both with when it was connected and when it ends, and all four go when it's
/// disconnected, which keeps the row. The default is on the hub's settings, where none is each machine's own login,
/// which every hub from before had. Changing it is a change to the settings' record, so it moves their revision.
pub(super) const GITHUB_ACCOUNTS: &str = r"
    CREATE TABLE github_accounts (
        id TEXT PRIMARY KEY,
        user_id INTEGER NOT NULL,
        username TEXT NOT NULL,
        login_kind TEXT,
        login_file TEXT,
        login_connected_at_ms INTEGER,
        login_ends_at_ms INTEGER,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER github_accounts_rev_on_insert AFTER INSERT ON github_accounts BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER github_accounts_rev_on_update AFTER UPDATE ON github_accounts WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    ALTER TABLE hub_settings ADD COLUMN github_account_id TEXT REFERENCES github_accounts(id);
";
