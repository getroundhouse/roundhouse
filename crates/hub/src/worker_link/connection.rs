use super::WorkerLinkId;
use super::credential::Following;
use super::{acks, reaching, reading};
use crate::calls::{Calls, NotSent};
use crate::child_process::{ChildProcess, Spawned};
use crate::error::internal;
use crate::hub::Hub;
use acks::Acks;
use protocol::Build;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{InstanceId, ProviderName, RunId, SessionId, WorkspaceId};
use protocol::worker::{
    AskAnswer, Reply, Request, RunningAgent, RunningAgents, Stop, WorkerCall, WorkerFrame, WorkerResponse,
};
use providers::Provider;
use reaching::{Opening, reach};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::time::Instant;
use tracing::warn;

/// How long a worker gets to stop its agents after its stdin closes. Agents get 3 s each.
pub(crate) const STOP_GRACE: Duration = Duration::from_secs(10);

pub(crate) struct WorkerLink {
    /// Distinguishes this connection from an earlier one to the same workspace.
    pub id: WorkerLinkId,
    /// Closing these closes the worker's stdin, which is how it's told to stop.
    pub(super) calls: Calls<Result<WorkerResponse, ApiError>>,
    pub(super) closed: watch::Sender<bool>,
    /// Something has wanted it since it was found running, so it's kept running.
    pub(super) kept: AtomicBool,
    /// The command the hub runs to reach the worker.
    pub(super) process: ChildProcess,
    /// What the worker has sent, but for its replies, while it's held, which [`WorkerLink::release`] ends. `None` once
    /// it has been, or if it never was. Held while each frame is passed on, so they go in the order they came.
    pub(super) held: AsyncMutex<Option<Vec<WorkerFrame>>>,
    pub(super) acks: Acks,
    /// Set as the hub leaves the worker running for the next hub, so its going isn't taken for the worker's.
    pub(super) left: AtomicBool,
    /// Which build the worker said it is, once it has said hello: `None` where its hello named none.
    pub(super) build: OnceLock<Option<String>>,
    /// The version of the protocol its hello chose, once it has said hello.
    pub(super) version: OnceLock<u32>,
}

fn lost() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the connection to the worker was lost")
}

/// A reply of another kind than its request is answered with, which only a worker that's wrong sends.
fn unexpected(reply: &WorkerResponse) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("unexpected reply from the worker: {reply:?}"))
}

impl WorkerLink {
    /// Starts the instance's worker, says hello and sets the hub's git credential, as [`WorkerLink::greet`] says, and
    /// from then on sets each the hub has, as [`Following::follow`] says. Then, on a worker on the hub's machine, asks
    /// where each repository's checkout whose address is to be read fetches from, as [`Hub::read_repo_urls`] says,
    /// without waiting for the answer. On failure nothing is left
    /// running, so the next attempt starts clean.
    pub(crate) async fn open(
        hub: Arc<Hub>,
        provider: Arc<dyn Provider>,
        workspace_id: WorkspaceId,
        instance_id: InstanceId,
        log: &std::path::Path,
        opening: Opening,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        let Spawned { process, reader, writer } = reach(
            provider.as_ref(),
            &instance_id,
            hub.worker_builds(),
            log,
            opening,
            hub.timers.before_serving,
            &hub.machine,
        )
        .await?;
        let link = Arc::new(WorkerLink {
            id: WorkerLinkId::next(),
            calls: Calls::new(writer),
            closed: watch::channel(false).0,
            kept: AtomicBool::new(false),
            process,
            held: AsyncMutex::new((opening == Opening::LeftRunning).then(Vec::new)),
            acks: Acks::default(),
            left: AtomicBool::new(false),
            build: OnceLock::new(),
            version: OnceLock::new(),
        });
        let within = hub.timers.hello_within;
        let left_to_machine = hub.leaves_github_to_machine(&ProviderName::from(provider.name()));
        let following = Following::new(link.clone(), (!left_to_machine).then(|| hub.git_credential()));
        tokio::spawn(acks::acknowledge(hub.clone(), link.clone()));
        tokio::spawn(reading::read_frames(
            hub.clone(),
            provider.clone(),
            link.clone(),
            workspace_id.clone(),
            instance_id.clone(),
            reader,
        ));

        let failure = match link.greet(within, following).await {
            Ok(following) => {
                tokio::spawn(following.follow());
                tokio::spawn(Arc::clone(&hub).read_repo_urls(workspace_id, Arc::clone(&link)));
                return Ok(link);
            }
            Err(failure) => failure,
        };
        link.kill();
        // A worker refused its instance says so, because another worker is serving it, which ending the instance would
        // end too. One left running is ended before the next worker starts, as the hub ends whatever it finds there.
        if opening == Opening::Started
            && failure.code != ErrorCode::Conflict
            && let Err(e) = provider.kill_worker(&instance_id).await
        {
            warn!(
                name: "worker.failed_start_ended",
                %instance_id,
                error = error_chain(&e),
                "Couldn't end a worker that failed to start"
            );
        }
        Err(failure)
    }

    /// The worker's reply, as what the request is answered with. A reply of another kind fails here, so a caller has
    /// only the reply it asked for to handle.
    pub(crate) async fn call<R: Request>(&self, request: R) -> Result<R::Reply, ApiError> {
        self.send(request).await?.await
    }

    /// Returns once the request is queued, with its reply to wait for, read as [`WorkerLink::call`] reads it. Requests
    /// reach the worker in the order they're queued. Dropping the reply forgets the call, so it no longer counts in
    /// [`WorkerLink::calls_under_way`], though the worker carries on with it.
    pub(crate) async fn send<R: Request>(
        &self,
        request: R,
    ) -> Result<impl Future<Output = Result<R::Reply, ApiError>> + use<R>, ApiError> {
        let request = request.into_request();
        self.send_call(|id| WorkerCall { id, request }).await
    }

    /// Sends the call `call` makes of its number, and returns once it's queued, with its reply, as `T`, to wait for.
    pub(super) async fn send_call<C: serde::Serialize, T: Reply, F: FnOnce(u64) -> C>(
        &self,
        call: F,
    ) -> Result<impl Future<Output = Result<T, ApiError>> + use<C, T, F>, ApiError> {
        let replied = self.calls.send(call).await.map_err(|e| match e {
            NotSent::Closed => lost(),
            NotSent::Encoding(e) => internal(e.to_string()),
        })?;
        Ok(async move { T::read(replied.await.map_err(|_| lost())??).map_err(|reply| unexpected(&reply)) })
    }

    /// Answers what the worker asked. Like a call, it's dropped if the connection has ended, since nobody is left to
    /// wait for it.
    pub(super) async fn answer(&self, answer: AskAnswer) {
        match self.calls.write(&answer).await {
            Ok(()) | Err(NotSent::Closed) => {}
            Err(NotSent::Encoding(e)) => {
                warn!(name: "worker.answer_unencoded", error = %e, "Couldn't encode an answer for a worker");
            }
        }
    }

    /// A link to no worker, on which every call fails as if the worker had gone.
    #[cfg(test)]
    pub(crate) fn unconnected() -> Arc<WorkerLink> {
        let calls = Calls::new(Box::new(tokio::io::sink()));
        calls.end();
        Arc::new(WorkerLink {
            id: WorkerLinkId::next(),
            calls,
            closed: watch::channel(true).0,
            kept: AtomicBool::new(false),
            process: ChildProcess::already_exited(),
            held: AsyncMutex::new(None),
            acks: Acks::default(),
            left: AtomicBool::new(false),
            build: OnceLock::new(),
            version: OnceLock::new(),
        })
    }

    /// Marks it wanted by something other than a read nobody asked for, so it isn't stopped after that read. Only
    /// where no such read can be deciding to stop it: before it's in the hub's map of workers, or while that's locked,
    /// which is where it's checked before one is stopped.
    pub(crate) fn keep(&self) {
        self.kept.store(true, Ordering::Relaxed);
    }

    /// Whether anything has wanted it, which one an earlier hub left running is not until something does.
    pub(crate) fn kept(&self) -> bool {
        self.kept.load(Ordering::Relaxed)
    }

    /// Whether the worker is another build than `hub`, as its hello said, such as one an earlier hub left. One that
    /// didn't say is from before workers said, so it's another too. An older build and a newer one both count, so a
    /// rollback replaces a worker too. Two builds without a stamp both say `-dev`, so they count as one build.
    pub(crate) fn is_another_build(&self, hub: Build) -> bool {
        self.build.get().is_some_and(|build| build.as_deref() != Some(hub.version()))
    }

    /// The build its hello named, for the log.
    pub(crate) fn build(&self) -> &str {
        self.said_build().unwrap_or("one from before workers said")
    }

    /// The version of the worker protocol its hello chose. `None` before its hello.
    pub(crate) fn version(&self) -> Option<u32> {
        self.version.get().copied()
    }

    /// The build its hello named. `None` before its hello, and where it named none.
    pub(crate) fn said_build(&self) -> Option<&str> {
        self.build.get().and_then(Option::as_deref)
    }

    /// How many calls to the worker wait for its reply.
    pub(crate) fn calls_under_way(&self) -> usize {
        self.calls.waiting()
    }

    /// Whether it's the workspace's worker in `workers`, rather than one that's been replaced or stopped.
    pub(crate) fn is_current(
        self: &Arc<Self>,
        workers: &HashMap<WorkspaceId, Arc<WorkerLink>>,
        id: &WorkspaceId,
    ) -> bool {
        workers.get(id).is_some_and(|current| Arc::ptr_eq(current, self))
    }

    pub(crate) fn is_open(&self) -> bool {
        !*self.closed.borrow() && self.calls.is_open()
    }

    /// Asks the worker to stop, and closes its stdin. It stops its agents, flushes, and exits.
    pub(crate) async fn close(&self) {
        // Not waited for, since its reply can't come before what's ahead of it, such as an agent's last output.
        let _ = self.send(Stop).await;
        self.calls.close();
    }

    /// Closes its stdin without asking the worker to stop, so it carries on for the next hub, once it has let go of
    /// what this hub has recorded, or `deadline` has passed.
    pub(crate) async fn leave(self: &Arc<Self>, hub: &Arc<Hub>, deadline: Instant) {
        self.left.store(true, Ordering::SeqCst);
        match tokio::time::timeout_at(deadline, self.acks.tell_and_wait(hub, self)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                warn!(
                    name: "worker.recorded_untold",
                    error = e.message,
                    "Couldn't tell a worker what was recorded as this hub leaves it"
                );
            }
            Err(_) => {
                warn!(
                    name: "worker.recorded_unheard",
                    "A worker didn't say in time that it had let go of what this hub recorded"
                );
            }
        }
        self.calls.close();
    }

    /// Asks the worker to stop and closes its stdin, then waits up to `grace` for the worker's command to exit, as it
    /// does once its agents have stopped, and ends it if it hasn't.
    pub(crate) async fn stop(&self, grace: Duration) {
        self.close().await;
        self.process.end(grace).await;
    }

    /// Closes its stdin and ends the worker's command at once. Its provider ends the worker.
    pub(crate) fn kill(&self) {
        self.calls.close();
        self.process.kill();
    }

    /// The agents the worker is running.
    pub(crate) async fn running_agents(&self) -> Result<Vec<RunningAgent>, ApiError> {
        Ok(self.call(RunningAgents).await?.agents)
    }

    /// Which run each session's agent is, by what the worker has sent of it while it's held, the newest if several.
    pub(crate) async fn held_runs(&self) -> HashMap<SessionId, RunId> {
        reading::newest_runs(self.held.lock().await.iter().flatten())
    }

    /// Sends on what the worker sent while it was held, and from then on as it comes.
    pub(crate) async fn release(&self, hub: &Arc<Hub>) {
        let mut held = self.held.lock().await;
        for frame in held.take().unwrap_or_default() {
            reading::route(hub, self, frame).await;
        }
    }

    /// Resolves once the worker's output has been fully read.
    pub(crate) async fn wait_closed(&self) {
        let mut closed = self.closed.subscribe();
        let _ = closed.wait_for(|done| *done).await;
    }
}

/// Which worker, for the log: the link's number, which tells two workers of one workspace apart, and its build.
impl fmt::Display for WorkerLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.said_build() {
            Some(build) => write!(f, "worker link {} of build {build}", self.id),
            None => write!(f, "worker link {}", self.id),
        }
    }
}
