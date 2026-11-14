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
        Self::stand_in(versions::WORKERS.newest, move |request| {
            answer(serde_json::from_value(request).expect("the hub sends requests"))
        })
    }

    /// The same, for a worker that chose protocol 3, whose requests `answer` reads as JSON, since this one's types
    /// can't read every one of them.
    pub(crate) fn answering_previous(
        answer: impl Fn(serde_json::Value) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
    ) -> Arc<WorkerLink> {
        Self::stand_in(protocol::worker::previous::VERSION, answer)
    }

    fn stand_in(
        version: u32,
        answer: impl Fn(serde_json::Value) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
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
            version: OnceLock::from(version),
        });
        let gone = Gone(Arc::downgrade(&link));
        tokio::spawn(async move {
            let mut calls = FrameReader::new(theirs);
            while let Ok(Some(line)) = calls.next().await {
                let Call { id, request } = frame::decode(line).expect("the hub sends calls");
                let Some(link) = gone.0.upgrade() else { return };
                if let Some(outcome) = answer(request) {
                    link.calls.reply(id, outcome);
                }
            }
        });
        link
    }
}

/// A call of any version, as each has them.
#[derive(serde::Deserialize)]
struct Call {
    id: u64,
    request: serde_json::Value,
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
