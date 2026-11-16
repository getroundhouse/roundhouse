//! The migration that keeps each sign-in with GitHub the hub runs, kept apart since it's long. It has its place in
//! `MIGRATIONS`.

/// Each sign-in with GitHub, by the ID its client chose, as a record the feed announces, with where it has got to as
/// `GithubSignInStatus`'s JSON. What GitHub is asked with is never here: the hub holds it in memory while the sign-in
/// waits, so a hub that starts removes every sign-in the last left. A GitHub account's login says whether it needs
/// signing in again, which every login from before doesn't.
pub(super) const GITHUB_SIGN_INS: &str = r"
    CREATE TABLE github_sign_ins (
        id TEXT PRIMARY KEY,
        status TEXT NOT NULL,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER github_sign_ins_rev_on_insert AFTER INSERT ON github_sign_ins BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_sign_ins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER github_sign_ins_rev_on_update AFTER UPDATE ON github_sign_ins WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_sign_ins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER github_sign_ins_rev_on_delete AFTER DELETE ON github_sign_ins BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
    ALTER TABLE github_accounts ADD COLUMN login_needs_sign_in INTEGER NOT NULL DEFAULT 0;
";
