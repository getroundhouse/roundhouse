//! What the worker asks its hub, for its agents' calls to its tools, and the answers it waits for.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::SessionId;
use protocol::worker::{AskAnswer, AskId, AskOutcome, AskReply, WorkerAsk, WorkerFrame};
use tokio::sync::{mpsc, oneshot};

#[cfg(test)]
mod tests;

/// A hub answers in seconds, even when it reads GitHub for the answer, and one from before workers could ask never
/// does.
const ANSWER_TIMEOUT: Duration = Duration::from_mins(1);

pub(crate) struct Asks {
    /// How long to wait for an answer.
    patience: Duration,
    next: AtomicU64,
    /// `None` once the hub has gone, so nothing waits for an answer that won't come.
    waiting: Mutex<Option<HashMap<AskId, oneshot::Sender<AskOutcome>>>>,
    frames: mpsc::Sender<WorkerFrame>,
}

fn hub_gone() -> ApiError {
    ApiError::new(ErrorCode::Unavailable, "the worker lost its hub")
}

impl Asks {
    pub(crate) fn new(frames: mpsc::Sender<WorkerFrame>) -> Self {
        Self::waiting_at_most(ANSWER_TIMEOUT, frames)
    }

    fn waiting_at_most(patience: Duration, frames: mpsc::Sender<WorkerFrame>) -> Self {
        Self { patience, next: AtomicU64::new(1), waiting: Mutex::new(Some(HashMap::new())), frames }
    }

    /// Asks the hub, for the agent in `session_id`, and waits for its answer. Sending the ask counts as waiting, since
    /// the way to the hub is shared with everything the agents say.
    pub(crate) async fn ask(&self, session_id: SessionId, ask: WorkerAsk) -> Result<AskReply, ApiError> {
        let id = AskId::new(self.next.fetch_add(1, Ordering::Relaxed));
        let (answered, answer) = oneshot::channel();
        self.waiting.lock().as_mut().ok_or_else(hub_gone)?.insert(id, answered);
        // Forgets it however this ends, including when whoever asked stops waiting.
        let _forget = Forget { asks: self, id };
        let asking = async {
            self.frames.send(WorkerFrame::Ask { id, session_id, ask }).await.map_err(|_| hub_gone())?;
            answer.await.map_err(|_| hub_gone())
        };
        let Ok(answered) = tokio::time::timeout(self.patience, asking).await else {
            let message = format!("the hub didn't answer within {:?}", self.patience);
            return Err(ApiError::new(ErrorCode::Unavailable, message));
        };
        answered?.into_result()
    }

    /// Hands the hub's answer to whoever asked, if they're still waiting.
    pub(crate) fn answer(&self, answer: AskAnswer) {
        let waiting = self.waiting.lock().as_mut().and_then(|waiting| waiting.remove(&answer.answer));
        if let Some(waiting) = waiting {
            let _ = waiting.send(answer.outcome);
        }
    }

    /// The hub has gone, so whatever waits for an answer fails now, and so does anything asked from here on.
    pub(crate) fn close(&self) {
        self.waiting.lock().take();
    }
}

struct Forget<'a> {
    asks: &'a Asks,
    id: AskId,
}

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        if let Some(waiting) = self.asks.waiting.lock().as_mut() {
            waiting.remove(&self.id);
        }
    }
}
