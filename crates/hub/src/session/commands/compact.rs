//! Asking the agent to compact its conversation: refused when it can't be, and otherwise recorded, then passed on
//! as a prompt is.

use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::{Seq, SessionStatus};
use tokio::sync::oneshot;

use super::super::{LiveSession, LiveState, Origin};
use super::Asking;
use crate::hub::Hub;

impl LiveSession {
    /// Records that someone asked the agent to compact, and asks it, unless [`refused_compaction`] says it can't be.
    pub(super) async fn handle_compact(&self, keeping: Option<String>, reply: oneshot::Sender<Result<(), ApiError>>) {
        let recorded = match self.hub() {
            Ok(hub) => self.record_compaction(&hub, keeping).await.map(|event| (hub, event)),
            Err(e) => Err(e),
        };
        let (hub, (seq, keeping)) = match recorded {
            Ok(recorded) => recorded,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let _ = reply.send(Ok(()));
        self.deliver(&hub, seq, Asking::Compact { keeping }).await;
    }

    /// Where the request is recorded, and what it says to keep.
    async fn record_compaction(&self, hub: &Hub, keeping: Option<String>) -> Result<(Seq, Option<String>), ApiError> {
        self.can_be_asked(hub).await?;
        let keeping = keeping.as_deref().and_then(said);
        let asked = EventBody::CompactionAsked { keeping: keeping.clone() };
        let refused = |state: &LiveState| {
            refused_compaction(state.progress.status, state.progress.queue.idle_is_stale(), state.agent_opened)
        };
        let (event, _) = self.record_unless(Origin::Hub, asked, None, refused).await?;
        Ok((event.seq, keeping))
    }
}

/// Why the agent can't be asked to compact, if it can't: it's on a turn, or has a prompt it hasn't taken, since it
/// would only compact once that's done, which may be long after it was wanted, or it has had no conversation yet.
fn refused_compaction(status: SessionStatus, untaken: bool, opened: bool) -> Option<ApiError> {
    let why = if status.in_turn() {
        "the agent can't compact until it's done with its turn"
    } else if untaken {
        "the agent can't compact until it has taken the prompt it was sent"
    } else if !opened {
        "there's nothing to compact until the agent has had a turn"
    } else {
        return None;
    };
    Some(ApiError::new(ErrorCode::Conflict, why))
}

/// What to keep, said in words, so nothing but space is the same as saying nothing.
fn said(keeping: &str) -> Option<String> {
    let keeping = keeping.trim();
    (!keeping.is_empty()).then(|| keeping.to_string())
}

#[cfg(test)]
mod tests {
    use protocol::model::SessionStatus;

    use super::{refused_compaction, said};

    #[test]
    fn what_to_keep_is_its_words_and_only_space_is_nothing() {
        assert_eq!(said("  Keep the API design\n").as_deref(), Some("Keep the API design"));
        assert_eq!((said(""), said(" \n ")), (None, None));
    }

    #[test]
    fn compacting_is_refused_on_a_turn_or_with_a_prompt_on_its_way_or_before_any_conversation() {
        let refused = |status, untaken| refused_compaction(status, untaken, true).is_some();

        assert!(refused(SessionStatus::Running, false) && refused(SessionStatus::WaitingApproval, false));
        assert!(refused(SessionStatus::Idle, true), "a prompt it hasn't taken yet");
        assert!(!refused(SessionStatus::Idle, false) && !refused(SessionStatus::Interrupted, false));
        assert!(!refused(SessionStatus::Background, false), "work in the background isn't a turn");
        assert!(refused_compaction(SessionStatus::Idle, false, false).is_some(), "no conversation yet");
    }
}
