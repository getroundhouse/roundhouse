use rusqlite::ErrorCode;

/// What a failure of SQLite says of the file, which decides what the hub does about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// Another connection held it for longer than the busy timeout, as the hub before this one can as it stops. It's
    /// left as it is.
    Busy,
    /// SQLite can't read it as a database, since it isn't one, or it's damaged. It's made again.
    Broken,
    /// Anything else, such as a table of a form this build can't make its own over, where emptying what failed and
    /// making it again can help.
    Other,
}

impl Failure {
    /// What `error` says of the file, by its primary code, which an extended code such as `SQLITE_BUSY_SNAPSHOT`
    /// shares.
    pub(super) fn of(error: &rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Busy,
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => Self::Broken,
            // Any other code, and an error that isn't SQLite's, is a failure of what's in the file, or of the disk it's
            // on.
            _ => Self::Other,
        }
    }
}
