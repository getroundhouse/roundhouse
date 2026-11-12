//! Waiting for the agent to pick up again by itself once the last of its work in the background has gone between
//! turns, which the session stays at work for, as [`super::status::after_background`] says. In case it never does, as
//! a CLI that changed might not, the hub takes the agent to be idle after a while, so the session isn't left at work
//! until the next prompt, keeping the machine awake all the while.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::event::{AgentRunState, EventBody};
use protocol::model::SessionStatus;
use tracing::{error, warn};

use super::{Item, LiveSession, LiveState, Origin};

/// How long the agent has to pick up again. The CLI does within a tenth of a second. An agent asked to stop says its
/// work was stopped too, and exits within eight seconds, three before it's killed and five for its last output, so its
/// exit comes first, and says it was cut short.
pub(crate) const PICKS_UP_WITHIN: Duration = Duration::from_secs(15);

impl LiveSession {
    /// Takes the agent to be idle if the session is still waiting, [`PICKS_UP_WITHIN`] after the wait began.
    pub(super) fn wait_for_pickup(&self) {
        let ingest = self.ingest.clone();
        tokio::spawn(async move {
            tokio::time::sleep(PICKS_UP_WITHIN).await;
            let _ = ingest.send(Item::PickupDue).await;
        });
    }

    /// Records, from the hub, that the agent is idle, unless the session has stopped waiting, as anything the agent
    /// says next ends the wait. A prompt it has yet to take ends it too, since taking that starts a turn. One timer can
    /// end a later wait than its own, which matters only if it comes in the moment before the agent picks up again.
    pub(super) async fn no_pickup(&self) {
        let idle = EventBody::AgentState { state: AgentRunState::Idle };
        let ended = |state: &LiveState| {
            let waiting = state.status == SessionStatus::Background
                && state.background_tasks.is_empty()
                && !state.queue.idle_is_stale();
            (!waiting).then(|| ApiError::new(ErrorCode::Conflict, "the agent has said something since"))
        };
        match self.record_unless(Origin::Hub, idle, None, ended).await {
            Ok(_) => {
                warn!(session = %self.id, "the agent didn't pick up again by itself after its work in the background");
            }
            Err(e) if e.code == ErrorCode::Conflict => {}
            Err(e) => error!(session = %self.id, "could not record the agent as idle: {e}"),
        }
    }
}
