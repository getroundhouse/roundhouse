//! Checking a decision on a request for approval before it's recorded and passed on to the agent that asked.

use super::LiveSession;
use protocol::capability::Capabilities;
use protocol::capability::approvals::{Decision, Request};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::ApprovalId;

impl LiveSession {
    /// What the request `approval` asks, as the log has it.
    pub(super) async fn approval_request(&self, approval: &ApprovalId) -> Result<Request, ApiError> {
        let (id, asked) = (self.id.clone(), approval.clone());
        self.hub()?.blocking(move |store| store.approval_request(&id, &asked)).await?.ok_or_else(|| {
            ApiError::new(ErrorCode::Conflict, format!("no approval {approval} was asked for in this chat"))
        })
    }
}

/// Why `decision` on `request` is refused, if it is: it's of a kind this build doesn't know, which a newer client sent,
/// it doesn't answer what was asked, as `Decision::answers` says, or the run that asked, which claims `claims`, can't
/// take it. A run that has said no claims takes none.
pub(super) fn refusal(request: &Request, decision: &Decision, claims: Option<&Capabilities>) -> Option<ApiError> {
    if *decision == Decision::Unknown {
        return Some(ApiError::new(ErrorCode::Unsupported, "this hub doesn't know that kind of decision"));
    }
    if !decision.answers(request) {
        return Some(ApiError::new(ErrorCode::InvalidRequest, "that doesn't answer what the agent asked"));
    }
    let takes = claims.is_some_and(|claims| decision.allowed(claims));
    (!takes).then(|| ApiError::new(ErrorCode::Unsupported, "the agent that asked can't take that decision"))
}
