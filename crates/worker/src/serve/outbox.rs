//! What agents say, on its way to the hub. It goes to the hub connected, if one is. For a hub that asked the worker to
//! keep running without it, each frame the hub records is also numbered, and kept until the hub says it has recorded
//! it, so the next hub to connect gets whatever the last one didn't. A delta is never kept, since it isn't recorded,
//! and the message it's part of comes whole once it's finished.

use std::collections::VecDeque;
use std::sync::Arc;

use protocol::worker::WorkerFrame;
use tokio::sync::{Mutex, mpsc};

#[derive(Default)]
pub(crate) struct Outbox(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    hub: Option<mpsc::Sender<Arc<WorkerFrame>>>,
    /// Whether to number and keep what's sent, as the last hub to say hello asked.
    keeping: bool,
    /// The number the last frame kept was given. The first is 1.
    numbered: u64,
    /// What the hub hasn't said it recorded, oldest first, each with its number.
    kept: VecDeque<(u64, Arc<WorkerFrame>)>,
}

impl Outbox {
    /// Sends `frame` to the hub connected, after everything sent before it, and numbers and keeps it first if the hub
    /// asked. Under one lock, so frames reach the hub in the order they were numbered.
    pub(crate) async fn send(&self, mut frame: WorkerFrame) {
        let mut inner = self.0.lock().await;
        let number = inner.keeping.then(|| number_of(&mut frame)).flatten().map(|seq| {
            inner.numbered += 1;
            *seq = Some(inner.numbered);
            inner.numbered
        });
        let frame = Arc::new(frame);
        if let Some(number) = number {
            inner.kept.push_back((number, frame.clone()));
        }
        if let Some(hub) = &inner.hub
            && hub.send(frame).await.is_err()
        {
            inner.hub = None;
        }
    }

    /// Lets go of what the hub has recorded, which is every frame through the one numbered `through`.
    pub(crate) async fn received(&self, through: u64) {
        let mut inner = self.0.lock().await;
        while inner.kept.front().is_some_and(|(number, _)| *number <= through) {
            inner.kept.pop_front();
        }
    }

    /// Sends `hub` whatever is kept, oldest first, and then each frame as it's sent, numbered and kept if `keeping`.
    pub(crate) async fn attach(&self, hub: mpsc::Sender<Arc<WorkerFrame>>, keeping: bool) {
        let mut inner = self.0.lock().await;
        inner.keeping = keeping;
        for (_, frame) in &inner.kept {
            if hub.send(frame.clone()).await.is_err() {
                return;
            }
        }
        inner.hub = Some(hub);
    }

    /// So nothing more goes to the hub that was connected.
    pub(crate) async fn detach(&self) {
        self.0.lock().await.hub = None;
    }
}

/// Where the frame's number goes, if it's one the hub records.
fn number_of(frame: &mut WorkerFrame) -> Option<&mut Option<u64>> {
    match frame {
        WorkerFrame::AgentEvent { seq, .. }
        | WorkerFrame::AgentContext { seq, .. }
        | WorkerFrame::AgentCache { seq, .. }
        | WorkerFrame::AgentLimits { seq, .. } => Some(seq),
        // A reply is only for the hub that asked, an ask is asked again of each hub until it's answered, the branches
        // are read again for each hub that watches them, and what an agent is writing is sent again to each hub that
        // connects.
        WorkerFrame::AgentDelta { .. }
        | WorkerFrame::AgentTail { .. }
        | WorkerFrame::Reply { .. }
        | WorkerFrame::Ask { .. }
        | WorkerFrame::Branches { .. } => None,
    }
}

#[cfg(test)]
mod tests;
