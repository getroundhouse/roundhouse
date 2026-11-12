//! The hub's end of one worker connection.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, Outcome, error_chain};
use protocol::frame;
use protocol::model::{InstanceId, SessionId, WorkspaceId};
use protocol::worker::{AskAnswer, AskId, WorkerAsk, WorkerCall, WorkerFrame, WorkerRequest, WorkerResponse};
use providers::Provider;
use tokio::sync::watch;
use tracing::{debug, warn};

use crate::calls::{Calls, NotSent};
use crate::child_process::{self, ChildProcess, Spawned, Stderr};
use crate::error::{internal, provider_failed};
use crate::frames::FrameReader;
use crate::hub::Hub;
use crate::session::Item;
use crate::versions;

static NEXT_LINK_ID: AtomicU64 = AtomicU64::new(1);

/// A worker on this machine answers in milliseconds. Hitting this means something is wrong with it.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a worker gets to stop its agents after its stdin closes. Agents get 3 s each.
pub(crate) const STOP_GRACE: Duration = Duration::from_secs(10);

pub(crate) struct WorkerLink {
    /// Distinguishes this connection from an earlier one to the same workspace.
    pub id: u64,
    /// Closing these closes the worker's stdin, which is how it's told to stop.
    calls: Calls<Result<WorkerResponse, ApiError>>,
    closed: watch::Sender<bool>,
    /// Something other than a read nobody asked for has wanted it, so it's kept running.
    kept: AtomicBool,
    /// The command the hub runs to reach the worker.
    process: ChildProcess,
    /// It didn't serve its tools when asked, as a worker that doesn't know how never answers, so its agents go without
    /// them from then on rather than each waiting for it.
    tools_unserved: AtomicBool,
}

fn lost() -> ApiError {
    ApiError::new(ErrorCode::Worker, "the connection to the worker was lost")
}

fn unreadable_reply(error: &serde_json::Error) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("a reply from the worker this hub can't read: {error}"))
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
    ) -> Result<Arc<WorkerLink>, ApiError> {
        // The local provider runs the hub's own executable, so there are no other builds of the worker to give it.
        provider.start(&instance_id, &[]).await.map_err(|e| provider_failed(&e))?;
        let connected = match provider.connect(&instance_id).await {
            Ok(command) => child_process::spawn(&command.program, &command.args, &command.env, Stderr::Log(log))
                .map_err(|e| ApiError::new(ErrorCode::Worker, e.to_string())),
            Err(e) => Err(provider_failed(&e)),
        };
        let Spawned { process, reader, writer } = match connected {
            Ok(spawned) => spawned,
            Err(e) => {
                let _ = provider.kill(&instance_id).await;
                return Err(e);
            }
        };
        let link = Arc::new(WorkerLink {
            id: NEXT_LINK_ID.fetch_add(1, Ordering::Relaxed),
            calls: Calls::new(writer),
            closed: watch::channel(false).0,
            kept: AtomicBool::new(false),
            process,
            tools_unserved: AtomicBool::new(false),
        });
        tokio::spawn(read_frames(hub, provider.clone(), link.clone(), workspace_id, instance_id.clone(), reader));

        let hello = link.call(WorkerRequest::Hello {
            protocol_version: versions::WORKERS.newest,
            oldest_protocol_version: Some(versions::WORKERS.oldest),
        });
        let failure = match tokio::time::timeout(HELLO_TIMEOUT, hello).await {
            Ok(Ok(WorkerResponse::Hello { protocol_version })) if versions::WORKERS.speaks(protocol_version) => {
                return Ok(link);
            }
            Ok(Ok(WorkerResponse::Hello { protocol_version })) => ApiError::new(
                ErrorCode::VersionMismatch,
                format!("the worker chose protocol {protocol_version}, and this hub speaks {}", versions::WORKERS),
            ),
            Ok(Ok(other)) => ApiError::new(ErrorCode::Worker, format!("unexpected hello reply: {other:?}")),
            Ok(Err(e)) => e,
            Err(_) => ApiError::new(ErrorCode::Worker, format!("the worker didn't answer within {HELLO_TIMEOUT:?}")),
        };
        link.kill();
        // A worker refused its instance says so, because another worker is serving it, which ending the instance would
        // end too.
        if failure.code != ErrorCode::Conflict
            && let Err(e) = provider.kill(&instance_id).await
        {
            warn!(%instance_id, "stopping a worker that failed to start: {}", error_chain(&e));
        }
        Err(failure)
    }

    pub(crate) async fn call(&self, request: WorkerRequest) -> Result<WorkerResponse, ApiError> {
        self.send(request).await?.await
    }

    /// Returns once the request is queued, with its reply to wait for. Requests reach the worker in the order
    /// they're queued.
    pub(crate) async fn send(
        &self,
        request: WorkerRequest,
    ) -> Result<impl Future<Output = Result<WorkerResponse, ApiError>> + use<>, ApiError> {
        let replied = self.calls.send(|id| WorkerCall { id, request }).await.map_err(|e| match e {
            NotSent::Closed => lost(),
            NotSent::Encoding(e) => internal(e.to_string()),
        })?;
        Ok(async move { replied.await.map_err(|_| lost())? })
    }

    /// Answers what the worker asked. Like a call, it's dropped if the connection has ended, since nobody is left to
    /// wait for it.
    async fn answer(&self, answer: AskAnswer) {
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

    /// Closes the worker's stdin. It stops its agents, flushes, and exits.
    pub(crate) fn close(&self) {
        self.calls.close();
    }

    /// Closes its stdin, then waits up to `grace` for the worker's command to exit, as it does once its agents have
    /// stopped, and ends it if it hasn't.
    pub(crate) async fn stop(&self, grace: Duration) {
        self.close();
        self.process.end(grace).await;
    }

    /// Closes its stdin and ends the worker's command at once.
    pub(crate) fn kill(&self) {
        self.close();
        self.process.kill();
    }

    /// Resolves once the worker's output has been fully read.
    pub(crate) async fn wait_closed(&self) {
        let mut closed = self.closed.subscribe();
        let _ = closed.wait_for(|done| *done).await;
    }
}

/// Answers what the worker asked on a task of its own, since answering can take a call to the worker, whose reply
/// comes the way the ask did.
fn start_answering(
    hub: &Arc<Hub>,
    link: &Arc<WorkerLink>,
    workspace_id: &WorkspaceId,
    id: AskId,
    session_id: SessionId,
    ask: WorkerAsk,
) {
    let (hub, link, workspace_id) = (hub.clone(), link.clone(), workspace_id.clone());
    tokio::spawn(async move {
        let outcome = hub.answer_ask(&workspace_id, &link, session_id, ask).await.into();
        link.answer(AskAnswer { answer: id, outcome }).await;
    });
}

/// Refuses an ask, as a call this hub can't read is refused, so the agent that called the tool hears why at once.
fn refuse_unreadable(link: &Arc<WorkerLink>, id: AskId, error: &serde_json::Error) {
    let outcome = Outcome::Error(ApiError::unsupported_request("this hub", Some(error)));
    let link = link.clone();
    tokio::spawn(async move { link.answer(AskAnswer { answer: id, outcome }).await });
}

async fn read_frames(
    hub: Arc<Hub>,
    provider: Arc<dyn Provider>,
    link: Arc<WorkerLink>,
    workspace_id: WorkspaceId,
    instance_id: InstanceId,
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
) {
    let mut incoming = FrameReader::new(reader);
    loop {
        let bytes = match incoming.next().await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(e) => {
                warn!("reading from a worker failed: {}", error_chain(&e));
                break;
            }
        };
        let parsed: WorkerFrame = match frame::decode(bytes) {
            Ok(parsed) => parsed,
            Err(e) => {
                // A reply from a newer worker, as after a rollback, ends its call rather than leaving it to wait until
                // the connection closes.
                if let Some(id) = frame::reply_id(bytes) {
                    link.calls.reply(id, Err(unreadable_reply(&e)));
                } else if let Some(id) = AskId::of_unreadable(bytes) {
                    refuse_unreadable(&link, id, &e);
                } else {
                    warn!("ignoring a worker frame that didn't parse: {e}");
                }
                continue;
            }
        };
        match parsed {
            WorkerFrame::Reply { id, outcome } => link.calls.reply(id, outcome.into_result()),
            WorkerFrame::AgentEvent { session_id, run, body, raw, attachments } => {
                hub.route(&session_id, Item::Event { run, body, raw, attachments }).await;
            }
            WorkerFrame::AgentDelta { session_id, run, delta } => {
                hub.route(&session_id, Item::Delta { run, delta }).await;
            }
            WorkerFrame::AgentContext { session_id, run, context } => {
                hub.route(&session_id, Item::Context { run, context }).await;
            }
            WorkerFrame::AgentCache { session_id, run, request } => {
                hub.route(&session_id, Item::Cache { run, request }).await;
            }
            WorkerFrame::AgentLimits { session_id, run, limits } => {
                hub.route(&session_id, Item::Limits { run, limits }).await;
            }
            WorkerFrame::Ask { id, session_id, ask } => {
                start_answering(&hub, &link, &workspace_id, id, session_id, ask);
            }
            WorkerFrame::Branches { branches } => hub.branches_read(&workspace_id, &link, branches).await,
        }
    }
    debug!(%instance_id, "worker connection ended");
    // Wakes each waiting call with an error.
    link.calls.end();
    hub.worker_gone(&workspace_id, &link, provider.as_ref(), &instance_id).await;
    // With `send_replace`, because nobody may be waiting yet. A plain `send` with no receivers leaves
    // the value unset, and whoever waits later would wait forever.
    link.closed.send_replace(true);
}

#[cfg(test)]
mod stand_in;
#[cfg(test)]
#[cfg(unix)]
mod tests;
