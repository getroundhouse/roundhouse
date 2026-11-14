//! The current run's claims authorize controls. Compatibility requests are only for runs that reported no claims.

use super::LiveSession;
use crate::worker_link::WorkerLink;
use protocol::capability::{Capabilities, Control};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::AgentKind;
use protocol::worker::{AgentControl, Delivery, Request};

impl LiveSession {
    pub(super) fn run_claims(&self) -> Option<Capabilities> {
        let state = self.state.lock();
        let run = state.agent?.run;
        state.capabilities.as_ref().filter(|reported| reported.run == run).map(|reported| (*reported.claims).clone())
    }

    pub(super) async fn control_agent<R: Request<Reply = Delivery>>(
        &self,
        link: &WorkerLink,
        control: Control,
        legacy: R,
    ) -> Result<Delivery, ApiError> {
        if let Some(claims) = self.run_claims() {
            if !control.allowed(&claims) {
                return Err(ApiError::new(ErrorCode::Unsupported, "this run does not offer that operation"));
            }
            return link.call(AgentControl { session_id: self.id.clone(), control }).await;
        }
        // A first plain prompt requires no claims, and can precede Claude's initialization. A legacy run never
        // reports claims; its old controls remain available without treating kind defaults as a run's report.
        if control.allowed(&Capabilities::default()) {
            match link.call(AgentControl { session_id: self.id.clone(), control }).await {
                Err(e) if e.code == ErrorCode::Unsupported && self.agent == AgentKind::Claude => {}
                delivered => return delivered,
            }
        } else if self.agent != AgentKind::Claude {
            return Err(ApiError::new(ErrorCode::Unsupported, "the run has not reported its capabilities"));
        }
        link.call(legacy).await
    }
}
