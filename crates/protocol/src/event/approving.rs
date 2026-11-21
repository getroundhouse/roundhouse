//! Which approval an event opens or closes.

use super::EventBody;
use crate::capability::approvals::Request;
use crate::model::ApprovalId;

/// What an event does to the approvals its agent waits on, as [`EventBody::approval_change`] says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalChange<'a> {
    /// It asks for the approval `approval_id`, which waits on the person from then until it closes.
    Opens { approval_id: &'a ApprovalId, request: &'a Request },
    /// It closes the approval: whoever decided it, or nobody, when it was cancelled.
    Closes(&'a ApprovalId),
}

impl ApprovalChange<'_> {
    /// The approval it opens or closes.
    pub fn approval_id(&self) -> &ApprovalId {
        match self {
            ApprovalChange::Opens { approval_id, .. } | ApprovalChange::Closes(approval_id) => approval_id,
        }
    }
}

impl EventBody {
    /// The approval it opens or closes, if it does either. The hub keeps what's open by this, and so do a client that
    /// shows it and an agent's adapter that answers it.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only `ApprovalRequested` and `ApprovalResolved` open or close an approval"
    )]
    pub fn approval_change(&self) -> Option<ApprovalChange<'_>> {
        match self {
            EventBody::ApprovalRequested { approval_id, request, .. } => {
                Some(ApprovalChange::Opens { approval_id, request })
            }
            EventBody::ApprovalResolved { approval_id, .. } => Some(ApprovalChange::Closes(approval_id)),
            _ => None,
        }
    }
}
