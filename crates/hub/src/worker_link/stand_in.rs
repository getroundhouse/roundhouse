//! A stand-in for a worker, for the hub's own tests of what it asks of one.

use std::sync::Weak;

use tokio::sync::oneshot;

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
        Self::answering_as(ChildProcess::already_exited(), answer)
    }

    /// The same, for a worker whose command exits only once it's killed, so stopping it takes all its grace.
    pub(crate) fn answering_until_killed(
        answer: impl Fn(WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
    ) -> Arc<WorkerLink> {
        Self::answering_as(ChildProcess::until_killed(), answer)
    }

    /// The same, for a worker that may answer a call only after it has answered ones that came later, as [`Answer`]
    /// says.
    pub(crate) fn answering_in_time(
        answer: impl Fn(WorkerRequest) -> Option<Answer> + Send + 'static,
    ) -> Arc<WorkerLink> {
        Self::stand_in(versions::WORKERS.newest, ChildProcess::already_exited(), move |request| {
            answer(serde_json::from_value(request).expect("the hub sends requests"))
        })
    }

    fn answering_as(
        process: ChildProcess,
        answer: impl Fn(WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
    ) -> Arc<WorkerLink> {
        Self::stand_in(versions::WORKERS.newest, process, move |request| {
            answer(serde_json::from_value(request).expect("the hub sends requests")).map(Answer::Now)
        })
    }

    /// The same link, as one whose hello named `build`, so a hub of another build takes it for another's worker.
    pub(crate) fn of_build(self: Arc<Self>, build: &str) -> Arc<Self> {
        self.build.set(Some(build.to_string())).expect("a stand-in says no hello");
        self
    }

    /// The same, for a worker that chose protocol 4, the version before this one.
    pub(crate) fn answering_previous(
        answer: impl Fn(WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + 'static,
    ) -> Arc<WorkerLink> {
        Self::stand_in(protocol::worker::previous::VERSION, ChildProcess::already_exited(), move |request| {
            answer(serde_json::from_value(request).expect("the hub sends requests")).map(Answer::Now)
        })
    }

    fn stand_in(
        version: u32,
        process: ChildProcess,
        answer: impl Fn(serde_json::Value) -> Option<Answer> + Send + 'static,
    ) -> Arc<WorkerLink> {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let link = Arc::new(WorkerLink {
            id: WorkerLinkId::next(),
            calls: Calls::new(Box::new(ours)),
            closed: watch::channel(false).0,
            kept: AtomicBool::new(false),
            process,
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
                match answer(request) {
                    None => {}
                    Some(Answer::Now(outcome)) => link.calls.reply(id, outcome),
                    Some(Answer::Later(outcome)) => {
                        let link = Arc::downgrade(&link);
                        tokio::spawn(async move {
                            if let (Ok(outcome), Some(link)) = (outcome.await, link.upgrade()) {
                                link.calls.reply(id, outcome);
                            }
                        });
                    }
                }
            }
        });
        link
    }
}

/// How a stand-in answers a call it doesn't leave unanswered.
pub(crate) enum Answer {
    /// At once, before it reads the next call.
    Now(Result<WorkerResponse, ApiError>),
    /// Once the test sends it, answering the calls after it meanwhile, so the hub can take in something else first, as
    /// it can while a real worker's answer is on its way.
    Later(oneshot::Receiver<Result<WorkerResponse, ApiError>>),
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
