//! The migrations that keep the logins the hub holds for agents' accounts, and which runs it launched under them,
//! kept apart since they're long. Each has its place in `MIGRATIONS`.

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

/// The account whose login the hub gave each run of a session's agent it launched under one, so which account the run
/// is under is known from the hub's own record, since a token doesn't say whose it is. Written before the launch is
/// sent, and removed once a worker says another run is the session's agent. Apart from the records, since no client
/// is shown it.
pub(super) const RUNS: &str = r"
    CREATE TABLE runs_under_logins (
        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        run INTEGER NOT NULL,
        account_id TEXT NOT NULL REFERENCES accounts(id),
        PRIMARY KEY (session_id, run)
    );
";
