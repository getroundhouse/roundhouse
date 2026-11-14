//! What a session asks of a worker of protocol 3, the version before this one, which goes with
//! [`protocol::worker::previous`], at the next break. A run of protocol 3 says what it claims as it
//! initializes, rather than as it launches, and a Claude run of a build from before claims never says, so it's sent the
//! requests that came before controls.

use super::LiveSession;
use crate::hub::Hub;
use crate::worker_link::WorkerLink;
use protocol::capability::{Capabilities, Control};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AgentKind, RunId, SessionId, SessionSettingsChange};
use protocol::worker::previous::{AgentCancel, AgentCompact, AgentConfigure, AgentPrompt, Request};
use protocol::worker::{AgentControl, Delivery};
use tracing::warn;

impl LiveSession {
    /// As [`LiveSession::control_agent`] asks a run of protocol 3 that hasn't said what it claims: only a plain prompt,
    /// which needs no claims, goes as a control, which a run that says them as it initializes takes before it has. A
    /// Claude run of a build from before claims is sent the request that came before controls.
    pub(super) async fn control_unclaimed(&self, link: &WorkerLink, control: Control) -> Result<Delivery, ApiError> {
        if control.allowed(&Capabilities::default()) {
            match link.call(AgentControl { session_id: self.id.clone(), control: control.clone() }).await {
                Err(e) if e.code == ErrorCode::Unsupported && self.agent == AgentKind::Claude => {}
                delivered => return delivered,
            }
        } else if self.agent != AgentKind::Claude {
            return Err(ApiError::new(ErrorCode::Unsupported, "the run has not reported its capabilities"));
        }
        let request = before_controls(self.id.clone(), control)
            .ok_or_else(|| ApiError::new(ErrorCode::Unsupported, "this run does not offer that operation"))?;
        link.call_previous(request).await
    }

    /// Changes the settings of a run of protocol 3 that hasn't said what it claims, with the request that came before
    /// controls.
    pub(super) async fn configure_previous(
        &self,
        link: &WorkerLink,
        options: SessionSettingsChange,
    ) -> Result<Delivery, ApiError> {
        link.call_previous(Request::AgentConfigure(AgentConfigure { session_id: self.id.clone(), options })).await
    }

    /// What `run`, of protocol 3, said it claims as it initialized, for a launch that finds it still running after the
    /// hub recorded its exit, as when its worker's connection dropped, which took them off the record. The log keeps
    /// them. Nothing when it can't be read, which leaves the run asked only what needs no claims.
    pub(super) async fn claims_said_before(&self, hub: &Hub, run: RunId) -> Option<Capabilities> {
        let id = self.id.clone();
        match hub.blocking(move |store| store.claims_said(&id, run)).await {
            Ok(claims) => claims,
            Err(e) => {
                warn!(session = %self.id, %run, "couldn't read what a run found running claims: {}", e.message);
                None
            }
        }
    }
}

/// `control` as the request that came before controls, for a run that hasn't said what it claims, if there was one.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "protocol 3's requests don't change, so no control added since has one"
)]
fn before_controls(session_id: SessionId, control: Control) -> Option<Request> {
    match control {
        Control::Prompt { seq, prompt, .. } => Some(Request::AgentPrompt(AgentPrompt { session_id, seq, prompt })),
        Control::Cancel => Some(Request::AgentCancel(AgentCancel { session_id })),
        Control::Compact { asked, keeping } => Some(Request::AgentCompact(AgentCompact { session_id, asked, keeping })),
        _ => None,
    }
}
