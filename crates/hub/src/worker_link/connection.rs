use super::{acks, reaching, reading};
use crate::calls::{Calls, NotSent};
use crate::child_process::{ChildProcess, Spawned};
use crate::error::internal;
use crate::hub::Hub;
use crate::versions;
use acks::Acks;
use protocol::Build;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{InstanceId, RunId, SessionId, WorkspaceId};
use protocol::worker::{
    AskAnswer, Hello, HelloReply, Received, Reply, Request, RunningAgent, RunningAgents, Stop, WorkerCall, WorkerFrame,
    WorkerResponse,
};
use providers::Provider;
use reaching::{Opening, reach};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, watch};
use tracing::warn;

pub(super) static NEXT_LINK_ID: AtomicU64 = AtomicU64::new(1);

/// A worker on this machine answers in milliseconds. Hitting this means something is wrong with it.
pub(super) const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a worker gets to stop its agents after its stdin closes. Agents get 3 s each.
pub(crate) const STOP_GRACE: Duration = Duration::from_secs(10);

pub(crate) struct WorkerLink {
    /// Distinguishes this connection from an earlier one to the same workspace.
    pub id: u64,
    /// Closing these closes the worker's stdin, which is how it's told to stop.
    pub(super) calls: Calls<Result<WorkerResponse, ApiError>>,
    pub(super) closed: watch::Sender<bool>,
    /// Something other than a read nobody asked for has wanted it, so it's kept running.
    pub(super) kept: AtomicBool,
    /// The command the hub runs to reach the worker.
    pub(super) process: ChildProcess,
    /// It didn't serve its tools when asked, as a worker that doesn't know how never answers, so its agents go without
    /// them from then on rather than each waiting for it.
    pub(super) tools_unserved: AtomicBool,
    /// What the worker has sent, but for its replies, while it's held, which [`WorkerLink::release`] ends. `None` once
    /// it has been, or if it never was. Held while each frame is passed on, so they go in the order they came.
    pub(super) held: AsyncMutex<Option<Vec<WorkerFrame>>>,
    pub(super) acks: Acks,
    /// Set as the hub leaves the worker running for the next hub, so its going isn't taken for the worker's.
    pub(super) left: AtomicBool,
    /// Which build the worker said it is, once it has said hello: `None` for a worker from before workers said.
    pub(super) build: OnceLock<Option<String>>,
}

fn lost() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the connection to the worker was lost")
}

/// A reply of another kind than its request is answered with, which only a worker that's wrong sends.
fn unexpected(reply: &WorkerResponse) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("unexpected reply from the worker: {reply:?}"))
}

impl WorkerLink {
    /// Starts the instance's worker and says hello. On failure nothing is left running, so the next
    /// attempt starts clean.
    pub(crate) async fn open(
        hub: Arc<Hub>,
        provider: Arc<dyn Provider>,
        workspace_id: WorkspaceId,
        instance_id: InstanceId,
        log: &std::path::Path,
        opening: Opening,
    ) -> Result<Arc<WorkerLink>, ApiError> {
        let Spawned { process, reader, writer } = reach(provider.as_ref(), &instance_id, log, opening).await?;
        let link = Arc::new(WorkerLink {
            id: NEXT_LINK_ID.fetch_add(1, Ordering::Relaxed),
            calls: Calls::new(writer),
            closed: watch::channel(false).0,
            kept: AtomicBool::new(false),
            process,
            tools_unserved: AtomicBool::new(false),
            held: AsyncMutex::new((opening == Opening::LeftRunning).then(Vec::new)),
            acks: Acks::default(),
            left: AtomicBool::new(false),
            build: OnceLock::new(),
        });
        tokio::spawn(acks::acknowledge(hub.clone(), link.clone()));
        tokio::spawn(reading::read_frames(
            hub,
            provider.clone(),
            link.clone(),
            workspace_id,
            instance_id.clone(),
            reader,
        ));

        let hello = link.call(Hello {
            protocol_version: versions::WORKERS.newest,
            oldest_protocol_version: Some(versions::WORKERS.oldest),
            // So a restart leaves it running, for the next hub. The hub says when it means it to stop.
            keep_running_without_hub: true,
        });
        let failure = match tokio::time::timeout(HELLO_TIMEOUT, hello).await {
            Ok(Ok(HelloReply { protocol_version, build })) if versions::WORKERS.speaks(protocol_version) => {
                let _ = link.build.set(build);
                return Ok(link);
            }
            Ok(Ok(HelloReply { protocol_version, .. })) => ApiError::new(
                ErrorCode::VersionMismatch,
                format!("the worker chose protocol {protocol_version}, and this hub speaks {}", versions::WORKERS),
            ),
            Ok(Err(e)) => e,
            Err(_) => ApiError::new(ErrorCode::Worker, format!("the worker didn't answer within {HELLO_TIMEOUT:?}")),
        };
        link.kill();
        // A worker refused its instance says so, because another worker is serving it, which ending the instance would
        // end too. One left running is ended before the next worker starts, as the hub ends whatever it finds there.
        if opening == Opening::Started
            && failure.code != ErrorCode::Conflict
            && let Err(e) = provider.kill(&instance_id).await
        {
            warn!(%instance_id, "stopping a worker that failed to start: {}", error_chain(&e));
        }
        Err(failure)
    }

    /// The worker's reply, as what the request is answered with. A reply of another kind fails here, so a caller has
    /// only the reply it asked for to handle.
    pub(crate) async fn call<R: Request>(&self, request: R) -> Result<R::Reply, ApiError> {
        self.send(request).await?.await
    }

    /// Returns once the request is queued, with its reply to wait for, read as [`WorkerLink::call`] reads it. Requests
    /// reach the worker in the order they're queued.
    pub(crate) async fn send<R: Request>(
        &self,
        request: R,
    ) -> Result<impl Future<Output = Result<R::Reply, ApiError>> + use<R>, ApiError> {
        let request = request.into_request();
        let replied = self.calls.send(|id| WorkerCall { id, request }).await.map_err(|e| match e {
            NotSent::Closed => lost(),
            NotSent::Encoding(e) => internal(e.to_string()),
        })?;
        Ok(async move { R::Reply::read(replied.await.map_err(|_| lost())??).map_err(|reply| unexpected(&reply)) })
    }

    /// Answers what the worker asked. Like a call, it's dropped if the connection has ended, since nobody is left to
    /// wait for it.
    pub(super) async fn answer(&self, answer: AskAnswer) {
        match self.calls.write(&answer).await {
            Ok(()) | Err(NotSent::Closed) => {}
            Err(NotSent::Encoding(e)) => warn!("could not encode an answer for a worker: {e}"),
        }
    }

    /// A link to no worker, on which every call fails as if the worker had gone.
    #[cfg(test)]
    pub(crate) fn unconnected() -> Arc<WorkerLink> {
        let calls = Calls::new(Box::new(tokio::io::sink()));
        calls.end();
        Arc::new(WorkerLink {
            id: NEXT_LINK_ID.fetch_add(1, Ordering::Relaxed),
            calls,
            closed: watch::channel(true).0,
            kept: AtomicBool::new(false),
            process: ChildProcess::already_exited(),
            tools_unserved: AtomicBool::new(false),
            held: AsyncMutex::new(None),
            acks: Acks::default(),
            left: AtomicBool::new(false),
            build: OnceLock::new(),
        })
    }

    /// Marks it wanted by something other than a read nobody asked for, so it isn't stopped after that read. Only
    /// where no such read can be deciding to stop it: before it's in the hub's map of workers, or while that's locked,
    /// which is where it's checked before one is stopped.
    pub(crate) fn keep(&self) {
        self.kept.store(true, Ordering::Relaxed);
    }

    /// Whether anything but a read nobody asked for has wanted it.
    pub(crate) fn kept(&self) -> bool {
        self.kept.load(Ordering::Relaxed)
    }

    /// Whether the worker is another build than `hub`, as its hello said, such as one an earlier hub left. One that
    /// didn't say is from before workers said, so it's another too.
    pub(crate) fn is_another_build(&self, hub: Build) -> bool {
        self.build.get().is_some_and(|build| build.as_deref() != Some(hub.version()))
    }

    /// The build its hello named, for the log.
    pub(crate) fn build(&self) -> &str {
        self.said_build().unwrap_or("one from before workers said")
    }

    /// The build its hello named. `None` before its hello, and from a worker from before workers said.
    pub(crate) fn said_build(&self) -> Option<&str> {
        self.build.get().and_then(Option::as_deref)
    }

    /// How many calls to the worker wait for its reply.
    pub(crate) fn calls_under_way(&self) -> usize {
        self.calls.waiting()
    }

    /// For a worker that didn't serve its tools when asked.
    pub(crate) fn tools_unserved(&self) {
        self.tools_unserved.store(true, Ordering::Relaxed);
    }

    /// Whether it didn't serve its tools when asked.
    pub(crate) fn serves_no_tools(&self) -> bool {
        self.tools_unserved.load(Ordering::Relaxed)
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

    /// Closes its stdin without asking the worker to stop, so it carries on for the next hub, once it has heard what
    /// this hub has recorded.
    pub(crate) async fn leave(self: &Arc<Self>, hub: &Arc<Hub>) {
        self.left.store(true, Ordering::SeqCst);
        if let Some(through) = self.acks.recorded(hub).await {
            let _ = self.send(Received { through }).await;
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
