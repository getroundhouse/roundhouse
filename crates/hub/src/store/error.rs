use std::io;
use std::path::PathBuf;

use protocol::client::{ApiError, ErrorCode, error_chain};
use tracing::error;

use crate::error::JsonProblem;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no {kind} {id}")]
    NotFound { kind: &'static str, id: String },
    #[error("the database was written by a newer version ({found} migrations, this build has {known})")]
    TooNew { found: usize, known: usize },
    /// Nothing was migrated, so `release` can still migrate it.
    #[error(
        "the database is older than {release}, the oldest release this build updates from ({found} migrations, where \
         {release} has {migrations}). Install {release} and run it once, then update"
    )]
    TooOld { found: usize, release: &'static str, migrations: usize },
    /// Nothing was migrated, so the database is as the last build left it.
    #[error("couldn't back up the database into {} before migrating it", dir.display())]
    Backup { dir: PathBuf, source: BackupError },
    #[error("the database failed")]
    Database(#[from] rusqlite::Error),
    #[error("a stored value couldn't be read or written")]
    Encoding(#[source] JsonProblem),
    /// A change to a chat's draft typed before the one the hub has, which stays, as `SessionDraft` says.
    #[error("the chat's draft was changed after this was typed, and that change is kept")]
    TypedEarlier,
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Encoding(error.into())
    }
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
            StoreError::TypedEarlier => ApiError::new(ErrorCode::Conflict, message),
            StoreError::TooNew { .. }
            | StoreError::TooOld { .. }
            | StoreError::Backup { .. }
            | StoreError::Database(_)
            | StoreError::Encoding(_) => {
                error!(name: "database.failed", error = message, "The database failed");
                ApiError::new(ErrorCode::Internal, message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use logging::testing::Logs;

    use super::*;
    use crate::store::columns::OneValue;
    use crate::store::database::json_from;

    /// The API error it becomes, and what that logged.
    fn convert(failure: StoreError) -> (ApiError, String) {
        let logs = Logs::capture();

        let api_error = ApiError::from(failure);

        (api_error, logs.text())
    }

    #[test]
    fn something_missing_stays_not_found_and_isnt_logged() {
        let (api_error, logged) = convert(StoreError::NotFound { kind: "session", id: "s1".to_string() });

        assert_eq!(api_error.code, ErrorCode::NotFound);
        assert_eq!(logged, "");
    }

    #[test]
    fn a_draft_typed_earlier_is_a_conflict_and_isnt_logged() {
        let (api_error, logged) = convert(StoreError::TypedEarlier);

        assert_eq!(api_error.code, ErrorCode::Conflict, "a client takes the hub's draft in place of its own");
        assert_eq!(logged, "");
    }

    /// Read as a column's JSON, and as JSON read some other way.
    #[test]
    fn what_a_stored_value_holds_isnt_logged() {
        let held = r#""a prompt someone wrote""#;
        let in_a_column = json_from::<u32>(OneValue, held).unwrap_err();
        let elsewhere = serde_json::from_str::<u32>(held).unwrap_err();

        for failure in [StoreError::from(in_a_column), StoreError::from(elsewhere)] {
            let (api_error, logged) = convert(failure);

            assert_eq!(api_error.code, ErrorCode::Internal);
            assert!(logged.contains("Data error at line 1, column 24"), "got {logged}");
            assert!(!logged.contains("someone wrote"), "got {logged}");
        }
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
