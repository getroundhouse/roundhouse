//! What the hub keeps of GitHub on its settings, and the GitHub accounts it holds, since the floor.

/// The providers whose machines Roundhouse leaves GitHub to, on the hub's settings, as a JSON list of their names, so
/// every client says nothing of whether the default can push from a workspace there. The hub writes it as it starts,
/// from its providers, before any client reads it, so the `NULL` every hub from before has is never read as of this
/// run. Changing it is a change to the settings' record, so it moves their revision.
pub(super) const GITHUB_LEFT_TO_MACHINE: &str = r"
    ALTER TABLE hub_settings ADD COLUMN github_left_to_machine TEXT;
";

/// The providers whose machines Roundhouse sets up GitHub on, on the hub's settings, as a JSON list of their names,
/// which the hub writes as it starts, before any client reads it. Changing it is a change to the settings' record, so
/// it moves their revision. And no table of the `gh` login each provider's machines had, which the hub asked their
/// workers for, since it asks no worker, and reads its own machine's only when a client asks.
pub(super) const GITHUB_SET_UP: &str = r"
    ALTER TABLE hub_settings ADD COLUMN github_set_up TEXT;
    DROP TABLE found_github_logins;
";

/// A GitHub account's removal moves the records' revision, as disconnecting any but an app's installation removes it,
/// so the feed's word that it's gone supersedes every copy of it. And no record of an account the person disconnected
/// before, with what GitHub's profile of its user said, nor the hub's settings' naming it as the default: an account
/// with no login that isn't an app's installation, whose record stays until its app is removed.
pub(super) const GITHUB_ACCOUNT_REMOVAL: &str = r"
    CREATE TRIGGER github_accounts_rev_on_delete AFTER DELETE ON github_accounts BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
    UPDATE hub_settings SET github_account_id = NULL WHERE github_account_id IN
        (SELECT id FROM github_accounts WHERE login_kind IS NULL AND app_id IS NULL);
    DELETE FROM github_profiles WHERE account_id IN
        (SELECT id FROM github_accounts WHERE login_kind IS NULL AND app_id IS NULL);
    DELETE FROM github_accounts WHERE login_kind IS NULL AND app_id IS NULL;
";

/// What GitHub calls each GitHub account's user, and what its profile says, which the hub keeps as observations, in its
/// cache file, and no longer here. Nothing of either is carried there: the hub asks GitHub again as it starts, since
/// one it doesn't know is due at once.
pub(super) const GITHUB_USERS_OBSERVED: &str = r"
    ALTER TABLE github_accounts DROP COLUMN username;
    DROP TABLE github_profiles;
";
