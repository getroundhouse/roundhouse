//! A stand-in for a worker, for the hub's own tests of what it asks of one.

use std::sync::Weak;

use protocol::frame;
use protocol::worker::WorkerRequest;

use super::*;
use crate::frames::FrameReader;

impl WorkerLink {
    /// A link to a stand-in for a worker, which answers each call with what `answer` says, in the order they come, or
    /// leaves it unanswered when that's `None`. Once the link is closed, or `answer` panics, it goes as a worker that
    /// exited would, so a test fails rather than waiting for ever.
    pub(crate) fn answering(
        answer: impl Fn(WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
    ) -> Arc<WorkerLink> {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let link = Arc::new(WorkerLink {
            id: NEXT_LINK_ID.fetch_add(1, Ordering::Relaxed),
            calls: Calls::new(Box::new(ours)),
            closed: watch::channel(false).0,
            kept: AtomicBool::new(false),
            process: ChildProcess::already_exited(),
            tools_unserved: AtomicBool::new(false),
            held: AsyncMutex::new(None),
            acks: Acks::default(),
            left: AtomicBool::new(false),
            build: OnceLock::new(),
        });
        let gone = Gone(Arc::downgrade(&link));
        tokio::spawn(async move {
            let mut calls = FrameReader::new(theirs);
            while let Ok(Some(line)) = calls.next().await {
                let call: WorkerCall = frame::decode(line).expect("the hub sends calls");
                let Some(link) = gone.0.upgrade() else { return };
                if let Some(outcome) = answer(call.request) {
                    link.calls.reply(call.id, outcome);
                }
            }
        });
        link
    }
}

/// Ends a stand-in's link when the stand-in stops, however it stops.
struct Gone(Weak<WorkerLink>);

impl Drop for Gone {
    fn drop(&mut self) {
        if let Some(link) = self.0.upgrade() {
            link.calls.end();
            link.closed.send_replace(true);
        }
    }
}
