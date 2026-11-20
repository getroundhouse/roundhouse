//! The migrations that keep the GitHub accounts the hub holds a login for, the logins found on each provider's
//! machines, whether the default can push to each repository, what GitHub's profile of each account's user says, and
//! what workers are sent for the default, kept apart since they're long. Each has its place in `MIGRATIONS`.

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

/// Who each provider's machines' own `gh` is logged in to github.com as, as a record the feed announces, by the
/// provider's name. `login` is JSON, as `GhLogin` has it. The hub deletes every row as it starts, since it asks again,
/// so none survives a restart. A change to a row moves its revision only when it finds another login, since the update
/// that writes it again leaves the row as it was.
pub(super) const FOUND_GITHUB_LOGINS: &str = r"
    CREATE TABLE found_github_logins (
        provider TEXT PRIMARY KEY,
        login TEXT NOT NULL,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER found_github_logins_rev_on_insert AFTER INSERT ON found_github_logins BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE found_github_logins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER found_github_logins_rev_on_update AFTER UPDATE ON found_github_logins WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE found_github_logins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";

/// Each app of your own the hub holds the key of, as a record the feed announces, by GitHub's number for it: what
/// GitHub calls it, who made it, its bot once GitHub has said its number, the file in the GitHub logins' directory its
/// key is in, as `crate::login_files` says, and the code GitHub made it from, which GitHub takes only once, so the same
/// code again is the same app. Removing it deletes its row, and its removal moves the revision, as a sign-in's does.
/// Each GitHub account that's an installation of one says which app, which installation, and the user or organization
/// it's on, which stay once its login has gone, as GitHub numbers and names them, and every account from before is
/// none.
pub(super) const GITHUB_APPS: &str = r"
    CREATE TABLE github_apps (
        id INTEGER PRIMARY KEY,
        slug TEXT NOT NULL,
        name TEXT NOT NULL,
        owner_id INTEGER NOT NULL,
        owner TEXT NOT NULL,
        bot_id INTEGER,
        bot TEXT,
        key_file TEXT NOT NULL,
        made_from TEXT NOT NULL UNIQUE,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER github_apps_rev_on_insert AFTER INSERT ON github_apps BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_apps SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER github_apps_rev_on_update AFTER UPDATE ON github_apps WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_apps SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER github_apps_rev_on_delete AFTER DELETE ON github_apps BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
    ALTER TABLE github_accounts ADD COLUMN app_id INTEGER;
    ALTER TABLE github_accounts ADD COLUMN installation_id INTEGER;
    ALTER TABLE github_accounts ADD COLUMN installed_on_id INTEGER;
    ALTER TABLE github_accounts ADD COLUMN installed_on TEXT;
";

/// Whether the default GitHub account can push to each repository's repository on GitHub, as the hub last checked, as
/// a record the feed announces, by the repository. `reach` is JSON, as `Reach` has it. `answered_at_ms` is when GitHub
/// gave that answer, by the hub's clock, and none for what isn't GitHub's answer. `stale` is a row the last run left,
/// which the hub hasn't checked again since it started. Kept across restarts, so a hub that starts again has it at
/// once, and knows which repository on GitHub each was checked for, which it checks again then. A change to a row moves its
/// revision only when it's another row, since the update that writes it again leaves the row as it was.
pub(super) const GITHUB_REACH: &str = r"
    CREATE TABLE github_reach (
        repo_id TEXT PRIMARY KEY REFERENCES repos(id),
        reach TEXT NOT NULL,
        answered_at_ms INTEGER,
        stale INTEGER NOT NULL DEFAULT 0,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER github_reach_rev_on_insert AFTER INSERT ON github_reach BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_reach SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER github_reach_rev_on_update AFTER UPDATE ON github_reach WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_reach SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";

/// What GitHub's profile of each GitHub account's user said as the hub last asked, which its commit identity is made
/// from: the user's name and public email, either of which GitHub can give as none, and when the hub asked. Only the
/// hub reads it, for what it sends workers, so it's apart from the accounts' rows and moves no revision. An account
/// with no row hasn't been read, as none from before was, so the hub's next check reads it. A row stays once the
/// account's login has gone, since it's of the user, and connecting the account again reads it again.
pub(super) const GITHUB_PROFILES: &str = r"
    CREATE TABLE github_profiles (
        account_id TEXT PRIMARY KEY REFERENCES github_accounts(id),
        name TEXT,
        email TEXT,
        read_at_ms INTEGER NOT NULL
    );
";

/// What the hub sends every worker for the default GitHub account, without its token, as it last worked it out, on the
/// hub's settings, as `SentCredential` writes it, so every client draws whether agents can push. The hub writes it as
/// it starts, before any client reads it, so the `NULL` every hub from before has is never read as of this run.
/// Changing it is a change to the settings' record, so it moves their revision.
pub(super) const GITHUB_SENT: &str = r"
    ALTER TABLE hub_settings ADD COLUMN github_sent TEXT;
";

/// The providers whose machines Roundhouse leaves GitHub to, on the hub's settings, as a JSON list of their names, so
/// every client says nothing of whether the default can push from a workspace there. The hub writes it as it starts,
/// from its providers, before any client reads it, so the `NULL` every hub from before has is never read as of this
/// run. Changing it is a change to the settings' record, so it moves their revision.
pub(super) const GITHUB_LEFT_TO_MACHINE: &str = r"
    ALTER TABLE hub_settings ADD COLUMN github_left_to_machine TEXT;
";
