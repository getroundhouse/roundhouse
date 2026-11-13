use std::io;
use std::path::PathBuf;

use protocol::client::{ApiError, ErrorCode, error_chain};
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no {kind} {id}")]
    NotFound { kind: &'static str, id: String },
    #[error("the database was written by a newer version ({found} migrations, this build has {known})")]
    TooNew { found: usize, known: usize },
    /// Nothing was migrated, so the database is as the last build left it.
    #[error("couldn't back up the database into {} before migrating it", dir.display())]
    Backup { dir: PathBuf, source: BackupError },
    #[error("the database failed")]
    Database(#[from] rusqlite::Error),
    #[error("a stored value couldn't be encoded")]
    Encoding(#[from] serde_json::Error),
}

/// Why a copy of the database couldn't be saved.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// What it was doing, and what went wrong.
    #[error("{what}")]
    Io { what: String, source: io::Error },
    #[error("copying the database with VACUUM INTO")]
    Vacuum(#[source] rusqlite::Error),
    #[error("moving what's in the WAL into the database before copying it")]
    Checkpoint(#[source] rusqlite::Error),
}

impl BackupError {
    pub(crate) fn io(what: impl Into<String>, source: io::Error) -> Self {
        Self::Io { what: what.into(), source }
    }
}

impl From<StoreError> for ApiError {
    fn from(failure: StoreError) -> Self {
        let message = error_chain(&failure);
        match failure {
            StoreError::NotFound { .. } => ApiError::new(ErrorCode::NotFound, message),
            StoreError::TooNew { .. }
            | StoreError::Backup { .. }
            | StoreError::Database(_)
            | StoreError::Encoding(_) => {
                error!("{message}");
                ApiError::new(ErrorCode::Internal, message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;

    use parking_lot::Mutex;

    use super::*;

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The API error it becomes, and what that logged.
    fn convert(failure: StoreError) -> (ApiError, String) {
        let captured = Captured::default();
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt().with_writer(move || writer.clone()).with_ansi(false).finish();

        let api_error = tracing::subscriber::with_default(subscriber, || ApiError::from(failure));

        let logged = String::from_utf8(captured.0.lock().clone()).unwrap();
        (api_error, logged)
    }

    #[test]
    fn something_missing_stays_not_found_and_isnt_logged() {
        let (api_error, logged) = convert(StoreError::NotFound { kind: "session", id: "s1".to_string() });

        assert_eq!(api_error.code, ErrorCode::NotFound);
        assert_eq!(logged, "");
    }

    #[test]
    fn a_database_failure_is_internal_and_logged_with_its_cause() {
        let (api_error, logged) = convert(StoreError::Database(rusqlite::Error::InvalidQuery));

        assert_eq!(api_error.code, ErrorCode::Internal);
        let cause = rusqlite::Error::InvalidQuery.to_string();
        assert!(api_error.message.contains(&cause), "got {}", api_error.message);
        assert!(logged.contains(&cause), "got {logged}");
    }
}
