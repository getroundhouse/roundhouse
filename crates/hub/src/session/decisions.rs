//! Checking a decision on a request for approval before it's recorded and passed on to the agent that asked.

use super::LiveSession;
use super::previous::unclaimed_takes;
use protocol::capability::Capabilities;
use protocol::capability::approvals::{Decision, Request};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AgentKind, ApprovalId};

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
/// it doesn't answer what was asked, as `Decision::answers` says, or the run of `agent` that asked, which claims
/// `claims`, can't take it. A run that claims nothing takes what [`unclaimed_takes`] says.
pub(super) fn refusal(
    request: &Request,
    decision: &Decision,
    claims: Option<&Capabilities>,
    agent: AgentKind,
) -> Option<ApiError> {
    if *decision == Decision::Unknown {
        return Some(ApiError::new(ErrorCode::Unsupported, "this hub doesn't know that kind of decision"));
    }
    if !decision.answers(request) {
        return Some(ApiError::new(ErrorCode::InvalidRequest, "that doesn't answer what the agent asked"));
    }
    let takes = match claims {
        Some(claims) => decision.allowed(claims),
        None => unclaimed_takes(agent, decision, request),
    };
    (!takes).then(|| ApiError::new(ErrorCode::Unsupported, "the agent that asked can't take that decision"))
}
