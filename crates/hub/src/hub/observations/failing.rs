//! The one way a change to an observation fails, a database that gives no revision, which the store logs itself, by
//! kind and by what it was doing, so a caller that would only log it says nothing.

use protocol::client::ApiError;
use tracing::warn;

use super::kept::Change;
use super::kinds::Kind;

/// Whether a change to an observation was kept. The store has logged it where it wasn't, so a caller that only wants
/// it kept drops this. One that acts on the failure, as a request refused for it, takes it with [`Keeping::result`].
/// It isn't a `Result`, which can't be dropped without ceremony, and no `?` takes it, so where a caller passed the
/// failure on before, it doesn't drop it by accident.
#[derive(Debug)]
pub(crate) struct Keeping(Result<(), ApiError>);

impl Keeping {
    /// Kept, or the failure the store logged.
    pub(crate) fn result(self) -> Result<(), ApiError> {
        self.0
    }
}

impl From<Result<(), ApiError>> for Keeping {
    fn from(kept: Result<(), ApiError>) -> Self {
        Self(kept)
    }
}

impl<V> Change<V> {
    /// What the change does, as the log says it.
    pub(super) fn doing(&self) -> &'static str {
        match self {
            Change::Read { .. } => "keeping a read",
            Change::WorkedOut { .. } => "keeping what it worked out",
            Change::Merged { .. } => "merging what it heard",
            Change::KeptBy { .. } => "keeping what keeps it",
            Change::NoLongerKeptBy { .. } => "letting go of what kept it",
            Change::NoLongerKeptWithItsKind { .. } => "letting go of its kind",
            Change::Gone => "forgetting it",
        }
    }
}

/// Logs that a change of kind `K` to the observation of `of`, `doing` what it does, failed with `error`, since the
/// database gave no revision. Its table and what it's of are as the cache file names them.
pub(super) fn logged<K: Kind>(of: &K::Of, doing: &'static str, error: &ApiError) {
    let of = serde_json::to_string(of).unwrap_or_default();
    warn!(
        name: "observation.unrevised",
        table = K::FORM.table,
        of,
        doing,
        error = error.message,
        "Couldn't change an observation, with no revision for it"
    );
}
