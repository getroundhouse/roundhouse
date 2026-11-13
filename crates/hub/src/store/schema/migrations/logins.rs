//! The migration that keeps the logins the hub holds for agents' accounts, kept apart since it's long. It has its place
//! in `MIGRATIONS`.

/// What an account's row says of a login the hub was given for it. `name` is what the person called the account, and
/// `login_agent` the kind of agent the login is for, which only an account whose ID the hub chose for a login has, and which
/// both stay once it's disconnected. `login_file` names the file in the logins directory its token is in, while the
/// hub holds it, as `crate::logins` says. When the login was connected and when it ends at the latest are there with
/// the file.
pub(super) const HELD: &str = r"
    ALTER TABLE accounts ADD COLUMN name TEXT;
    ALTER TABLE accounts ADD COLUMN login_agent TEXT;
    ALTER TABLE accounts ADD COLUMN login_file TEXT;
    ALTER TABLE accounts ADD COLUMN login_connected_at_ms INTEGER;
    ALTER TABLE accounts ADD COLUMN login_ends_at_ms INTEGER;
";
