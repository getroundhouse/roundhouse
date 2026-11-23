use rusqlite::ErrorCode;

/// What a failure of SQLite says of the file, which decides what the hub does about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// Another connection held it for longer than the busy timeout, as the hub before this one can as it stops. It's
    /// left as it is.
    Busy,
    /// SQLite can't read it as a database, since it isn't one, or it's damaged. It's made again.
    Broken,
    /// It can't be read or written now, which says nothing of what's in it, as on a full disk, a failure of the disk,
    /// or a file or directory the hub can't write. It's left as it is.
    Unusable,
    /// Anything else, such as a table of a form this build can't make its own over. A part whose tables fail so is
    /// emptied and made again, and a file whose own setup does is left as it is.
    Other,
}

impl Failure {
    /// What `error` says of the file, by its primary code, which an extended code such as `SQLITE_BUSY_SNAPSHOT`
    /// shares.
    pub(super) fn of(error: &rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Busy,
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => Self::Broken,
            Some(
                ErrorCode::PermissionDenied
                | ErrorCode::ReadOnly
                | ErrorCode::SystemIoFailure
                | ErrorCode::DiskFull
                | ErrorCode::CannotOpen
                | ErrorCode::OutOfMemory
                | ErrorCode::FileLockingProtocolFailed
                | ErrorCode::NoLargeFileSupport,
            ) => Self::Unusable,
            // Any other code, such as a table or a column this build doesn't know, and an error that isn't SQLite's.
            _ => Self::Other,
        }
    }
}
