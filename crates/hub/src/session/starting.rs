//! What a live session starts from, read from the store together.

use protocol::model::{ApprovalId, SessionId};

use super::progress::Failing;
use crate::store::{Store, StoreError, StoredSession};

/// What a live session starts from: the session's record, the approvals still open, and the failures in a row its log
/// ends in, as [`Failing::recorded`] says, read in one place so whatever starts one starts it from all of them.
pub(crate) struct Starting {
    pub(crate) stored: StoredSession,
    pub(crate) open_approvals: Vec<ApprovalId>,
    pub(super) failing: Option<Failing>,
}

impl Starting {
    pub(crate) fn read(store: &Store, id: &SessionId) -> Result<Self, StoreError> {
        let (stored, open_approvals) = (store.get_session(id)?, store.open_approvals(id)?);
        Ok(Self { stored, open_approvals, failing: Failing::recorded(store, id)? })
    }

    /// From `stored` alone, with no approval open and no failures in a row, as a test that makes the record by hand
    /// starts one.
    #[cfg(test)]
    pub(crate) fn of(stored: StoredSession) -> Self {
        Self { stored, open_approvals: Vec::new(), failing: None }
    }
}
