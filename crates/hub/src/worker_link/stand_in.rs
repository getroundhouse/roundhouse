//! A stand-in for a worker, for the hub's own tests of what it asks of one.

use std::sync::Weak;

use tokio::sync::oneshot;

use protocol::frame;
use protocol::model::WorkspaceId;
use protocol::worker::WorkerRequest;

use super::following::{Following, Servers};
use super::*;
use crate::frames::FrameReader;

impl WorkerLink {
    /// A link to a stand-in for a worker, which answers each call with what `answer` says, in the order they come, or
    /// leaves it unanswered when that's `None`. `answer` is given every call sent before the link went, even one it
    /// reads after. Once the link is closed, or `answer` panics, it goes as a worker that exited would, so a test fails
    /// rather than waiting for ever.
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

    /// Sets `hub`'s tool servers on the stand-in, as `workspace_id`'s worker, as they're set on a worker as its link opens
    /// and from then on, as [`WorkerLink::open`] says.
    pub(crate) async fn set_as_opened(self: &Arc<Self>, hub: &Arc<Hub>, workspace_id: WorkspaceId) {
        let mut servers = Following::new(Arc::clone(self), Servers::new(hub.sent_tool_servers()));
        let answer = servers.queue().await.unwrap();
        servers.answered(answer).await.unwrap();
        tokio::spawn(Arc::clone(hub).reconnect_as_changed(workspace_id, Arc::clone(self)));
        tokio::spawn(servers.follow());
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

    /// The same, for a worker that chose protocol 5, the version before this one.
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
            carrier: super::carrier::Carrier::Command(process),
            held: AsyncMutex::new(None),
            acks: Acks::default(),
            left: AtomicBool::new(false),
            build: OnceLock::new(),
            version: OnceLock::from(version),
            tool_servers: watch::Sender::default(),
        });
        let gone = Gone(Arc::downgrade(&link));
        tokio::spawn(async move {
            let mut calls = FrameReader::new(theirs);
            while let Ok(Some(line)) = calls.next().await {
                let Call { id, request } = frame::decode(line).expect("the hub sends calls");
                // Every call the hub sent reaches what stands in for the worker, as everything written to a worker's
                // stdin before it closed reaches the worker, though the link may have gone since, as the last call
                // to a worker the hub has let go of, `Stop`, often finds it. Only the reply has nowhere to go then.
                let answered = answer(request);
                let Some(link) = gone.0.upgrade() else { continue };
                match answered {
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

/// Every call the hub sends a stand-in reaches it, even once the link has gone, as the `Stop` the hub sends a worker
/// it has let go of does, with nothing waiting for its reply.
#[tokio::test]
async fn a_call_sent_before_its_link_goes_reaches_the_stand_in() {
    let asked = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let kept = asked.clone();
    let link = WorkerLink::answering(move |request| {
        kept.lock().push(request);
        Some(Ok(WorkerResponse::Ok))
    });

    link.close().await;
    drop(link);

    waiting::task::until(Duration::from_secs(10), "the stand-in to be asked to stop", || {
        asked.lock().iter().any(|request| matches!(request, WorkerRequest::Stop))
    })
    .await;
}
