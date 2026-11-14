//! What the hub asks of the session's run, which its claims allow, as its worker said them as it launched it.

use super::LiveSession;
use crate::worker_link::WorkerLink;
use protocol::capability::{Capabilities, Control};
use protocol::client::{ApiError, ErrorCode};
use protocol::worker::{AgentControl, Delivery};

impl LiveSession {
    /// What the run that's running claims, as it said, and nothing when none is.
    pub(super) fn run_claims(&self) -> Option<Capabilities> {
        let state = self.state.lock();
        let run = state.agent?.run;
        state.capabilities.as_ref().filter(|reported| reported.run == run).map(|reported| (*reported.claims).clone())
    }

    /// How the session's run, on `link`, is asked anything. A run of this protocol says its claims as it launches, so
    /// one with none said isn't asked, and with no run current, none is running.
    pub(super) fn askable(&self, link: &WorkerLink) -> Result<Askable, ApiError> {
        if let Some(claims) = self.run_claims() {
            return Ok(Askable::Claimed(Box::new(claims)));
        }
        if link.speaks_previous() {
            return Ok(Askable::Unclaimed);
        }
        if self.agent_running() {
            return Err(ApiError::new(ErrorCode::Unsupported, "the agent's run hasn't said what it can do"));
        }
        Ok(Askable::NotRunning)
    }

    /// Asks the session's run to do what `control` says, if it claims it can.
    pub(super) async fn control_agent(&self, link: &WorkerLink, control: Control) -> Result<Delivery, ApiError> {
        match self.askable(link)? {
            Askable::Claimed(claims) => self.control_claimed(link, control, &claims).await,
            Askable::Unclaimed => self.control_unclaimed(link, control).await,
            Askable::NotRunning => Ok(Delivery::AgentNotRunning),
        }
    }

    /// Asks the session's run, which claims `claims`, to do what `control` says, if they allow it.
    pub(super) async fn control_claimed(
        &self,
        link: &WorkerLink,
        control: Control,
        claims: &Capabilities,
    ) -> Result<Delivery, ApiError> {
        if !control.allowed(claims) {
            return Err(ApiError::new(ErrorCode::Unsupported, "this run does not offer that operation"));
        }
        link.call(AgentControl { session_id: self.id.clone(), control }).await
    }
}

/// How the session's run is asked anything, as [`LiveSession::askable`] says.
pub(super) enum Askable {
    /// By the claims it said.
    Claimed(Box<Capabilities>),
    /// As a run of protocol 3 that hasn't said them is, as [`LiveSession::control_unclaimed`] says.
    Unclaimed,
    /// It isn't, since none is running.
    NotRunning,
}
