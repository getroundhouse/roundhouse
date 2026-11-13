use protocol::model::SessionId;
use protocol::tail::{StreamDelta, Tail};
use protocol::worker::WorkerFrame;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

#[derive(Default)]
pub(crate) struct Outbox(pub(super) Mutex<Inner>);

#[derive(Default)]
pub(super) struct Inner {
    pub(super) hub: Option<mpsc::Sender<Arc<WorkerFrame>>>,
    /// Whether to number and keep what's sent, as the last hub to say hello asked.
    pub(super) keeping: bool,
    /// The number the last frame kept was given. The first is 1.
    pub(super) numbered: u64,
    /// What the hub hasn't said it recorded, oldest first, each with its number.
    pub(super) kept: VecDeque<(u64, Arc<WorkerFrame>)>,
    pub(super) writing: Writing,
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
        inner.writing.note(&frame);
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

    /// Sends `hub` whatever is kept, oldest first, then what each agent is writing, and then each frame as it's sent,
    /// numbered and kept if `keeping`.
    pub(crate) async fn attach(&self, hub: mpsc::Sender<Arc<WorkerFrame>>, keeping: bool) {
        let mut inner = self.0.lock().await;
        inner.keeping = keeping;
        let kept = inner.kept.iter().map(|(_, frame)| frame.clone());
        let catching_up: Vec<_> = kept.chain(inner.writing.frames().map(Arc::new)).collect();
        for frame in catching_up {
            if hub.send(frame).await.is_err() {
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

/// What each session's agent is writing, as the frames sent so far make it, whether or not a hub was there for them.
#[derive(Default)]
pub(super) struct Writing(pub(super) HashMap<SessionId, Tail>);

impl Writing {
    /// Takes in what `frame` changes of what an agent is writing.
    pub(super) fn note(&mut self, frame: &WorkerFrame) {
        match frame {
            WorkerFrame::AgentDelta { session_id, run, delta } => {
                let delta = StreamDelta { run: Some(*run), ..delta.clone() };
                self.change(session_id, |tail| tail.apply(&delta));
            }
            WorkerFrame::AgentEvent { session_id, run, body, .. } => {
                self.change(session_id, |tail| tail.on_agent_event(*run, body));
            }
            WorkerFrame::AgentContext { .. }
            | WorkerFrame::AgentCache { .. }
            | WorkerFrame::AgentLimits { .. }
            | WorkerFrame::AgentTail { .. }
            | WorkerFrame::Reply { .. }
            | WorkerFrame::Ask { .. }
            | WorkerFrame::Branches { .. }
            | WorkerFrame::ChildBranches { .. } => {}
        }
    }

    /// Makes `change` to what's being written in `session_id`, keeping it only while something is, so a session
    /// nothing is being written in costs nothing.
    pub(super) fn change(&mut self, session_id: &SessionId, change: impl FnOnce(&mut Tail)) {
        if let Some(tail) = self.0.get_mut(session_id) {
            change(tail);
            if tail.holds_nothing() {
                self.0.remove(session_id);
            }
            return;
        }
        let mut tail = Tail::default();
        change(&mut tail);
        if !tail.holds_nothing() {
            self.0.insert(session_id.clone(), tail);
        }
    }

    /// A frame for each run with something being written, each session's oldest run first.
    pub(super) fn frames(&self) -> impl Iterator<Item = WorkerFrame> + '_ {
        self.0.iter().flat_map(|(session_id, tail)| {
            let of_run = |run| WorkerFrame::AgentTail { session_id: session_id.clone(), run, tail: tail.of_run(run) };
            tail.runs().into_iter().map(of_run)
        })
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
        | WorkerFrame::Branches { .. }
        | WorkerFrame::ChildBranches { .. } => None,
    }
}
