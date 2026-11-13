//! One hub's connection to the worker: its calls in, and the replies and what agents say out.

use std::sync::Arc;

use protocol::Build;
use protocol::client::{ApiError, ErrorCode};
use protocol::frame::{self, Call};
use protocol::versions::Versions;
use protocol::worker::{
    Hello, HelloReply, ToWorker, WORKER_PROTOCOL_VERSION, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, warn};

use super::scripts::setup_requested;
use super::{Worker, handle};

/// Enough for a burst of what agents say, without holding much for a hub that's slow to read.
const QUEUED_FRAMES: usize = 1024;

/// What the hub said about losing it, which decides whether the worker carries on once it has gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    /// It asked the worker to stop, or never asked it to keep running without a hub.
    Stop,
    /// It asked the worker to keep running without a hub, and never to stop, or it went before saying hello.
    Lost,
}

/// A connection the hub has stopped sending on, which can still send the hub whatever is on its way.
pub(crate) struct Closing {
    #[cfg_attr(not(unix), expect(dead_code, reason = "only a worker on a socket carries on once its hub has gone"))]
    pub(crate) ended: Ended,
    frames: mpsc::Sender<Arc<WorkerFrame>>,
    writer: JoinHandle<()>,
}

impl Closing {
    /// Once every reply under way has gone, and everything else sent to it, such as what agents said as they stopped.
    /// The outbox has to have let go of it first.
    pub(crate) async fn finish(self) {
        drop(self.frames);
        let _ = self.writer.await;
    }

    /// Without what hasn't gone yet. The outbox keeps that for the next hub, if this one asked.
    #[cfg(unix)]
    pub(crate) fn abandon(self) {
        self.writer.abort();
    }
}

/// Until the hub stops sending. A worker that can't serve this hub, because of `refused`, answers every call by saying
/// so, and sends it nothing else.
pub(crate) async fn serve<R, W>(worker: &Worker, reader: R, writer: W, refused: Option<String>) -> Closing
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let (frames, outgoing) = mpsc::channel(QUEUED_FRAMES);
    let writer = tokio::spawn(write_frames(writer, outgoing));
    // `None` until the hub has said hello, and then whether it asked the worker to keep running without it.
    let mut keep_running = None;
    let mut asked_to_stop = false;
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    while let Some(line) = next_line(&mut reader, &mut line).await {
        let Call { id, request } = match line {
            ToWorker::Call(call) => call,
            // A refused hub was asked nothing.
            ToWorker::Answer(answer) => {
                if refused.is_none() {
                    worker.state.asks.answer(answer);
                }
                continue;
            }
        };
        let reply = move |answer: Result<WorkerResponse, ApiError>| {
            let outcome = answer.map_or_else(WorkerOutcome::Error, WorkerOutcome::Response);
            Arc::new(WorkerFrame::Reply { id, outcome })
        };
        if let Some(reason) = &refused {
            let _ = frames.send(reply(Err(ApiError::new(ErrorCode::Conflict, reason.clone())))).await;
            continue;
        }
        // Answered here rather than on a task of their own, since what the hub asks in them has to hold for every
        // call after, and when the connection ends, or happen in the order it was asked, as typing does.
        match request {
            Ok(WorkerRequest::Hello(asked)) => {
                keep_running = greet(worker, &frames, asked, reply).await.or(keep_running);
            }
            Ok(WorkerRequest::Stop) => {
                asked_to_stop = true;
                let _ = frames.send(reply(Ok(WorkerResponse::Ok))).await;
            }
            Ok(WorkerRequest::TerminalInput(typed)) => {
                let typed = worker.state.terminals.input(typed);
                let _ = frames.send(reply(typed.map(|()| WorkerResponse::Ok))).await;
            }
            Ok(WorkerRequest::TerminalResize(resize)) => {
                let resized = worker.state.terminals.resize(resize);
                let _ = frames.send(reply(resized.map(|()| WorkerResponse::Ok))).await;
            }
            request => {
                // Each call runs on a task of its own, so a stop read after this could otherwise be handled before the
                // setup it's for has begun.
                if matches!(request, Ok(WorkerRequest::SetupWorkspace(_))) {
                    setup_requested(&worker.state);
                }
                let (worker, frames) = (worker.clone(), frames.clone());
                tokio::spawn(async move {
                    let answer = handle(&worker.state, &worker.forwarders, request).await;
                    let _ = frames.send(reply(answer)).await;
                });
            }
        }
    }
    let ended = if asked_to_stop || keep_running == Some(false) { Ended::Stop } else { Ended::Lost };
    Closing { ended, frames, writer }
}

/// Answers the hub's hello, and once the worker speaks the hub's version, sends it what's on its way and says whether
/// it asked the worker to keep running without it.
async fn greet(
    worker: &Worker,
    frames: &mpsc::Sender<Arc<WorkerFrame>>,
    asked: Hello,
    reply: impl FnOnce(Result<WorkerResponse, ApiError>) -> Arc<WorkerFrame>,
) -> Option<bool> {
    let Hello { protocol_version, oldest_protocol_version, keep_running_without_hub } = asked;
    let answer = hello(worker.state.config.build, protocol_version, oldest_protocol_version);
    let greeted = answer.is_ok();
    let _ = frames.send(reply(answer)).await;
    if !greeted {
        return None;
    }
    // After the reply, so the hub knows which version this is before anything else reaches it.
    worker.state.outbox.attach(frames.clone(), keep_running_without_hub).await;
    worker.state.asks.ask_again().await;
    Some(keep_running_without_hub)
}

/// The next call the hub sends, or answer to what the worker asked, or `None` once it has stopped sending, or sent more
/// than a frame can hold.
async fn next_line<R: AsyncRead + Unpin>(reader: &mut BufReader<R>, line: &mut Vec<u8>) -> Option<ToWorker> {
    loop {
        frame::clear_buffer(line);
        let limit = u64::try_from(frame::MAX_FRAME_BYTES).unwrap_or(u64::MAX).saturating_add(1);
        match (&mut *reader).take(limit).read_until(b'\n', line).await {
            Ok(0) => return None,
            Ok(_) => {}
            Err(e) => {
                error!("reading from the hub failed: {e}");
                return None;
            }
        }
        if line.len() > frame::MAX_FRAME_BYTES {
            error!("frame from the hub is too large");
            return None;
        }
        match ToWorker::decode(line) {
            Ok(read) => return Some(read),
            Err(e) => warn!("ignoring a frame that didn't parse: {e}"),
        }
    }
}

async fn write_frames<W: AsyncWrite + Unpin>(mut writer: W, mut frames: mpsc::Receiver<Arc<WorkerFrame>>) {
    while let Some(item) = frames.recv().await {
        let line = match frame::encode(item.as_ref()) {
            Ok(line) => line,
            Err(e) => {
                error!("could not encode a frame: {e}");
                continue;
            }
        };
        if let Err(e) = writer.write_all(&line).await.and(writer.flush().await) {
            // The hub is gone. Keep draining, so agents stop cleanly rather than blocking on us.
            warn!("writing to the hub failed: {e}");
        }
    }
}

/// A worker speaks only its own version, and takes a hub whose hello names it, since a hub speaks the one before its
/// own too once a protocol breaks, as "Changing the protocol" in `docs/architecture.md` says.
fn hello(build: Build, newest: u32, oldest: Option<u32>) -> Result<WorkerResponse, ApiError> {
    let (ours, hub) = (Versions::only(WORKER_PROTOCOL_VERSION), Versions::from_hello(newest, oldest));
    let Some(chosen) = ours.newest_shared(hub) else {
        let message = format!("the hub speaks {hub}, and this worker speaks {ours}");
        return Err(ApiError::new(ErrorCode::VersionMismatch, message));
    };
    Ok(WorkerResponse::Hello(HelloReply { protocol_version: chosen, build: Some(said(build)) }))
}

/// Which build this worker is, as its hello says. A test can have it say another, with [`TEST_BUILD`], as a worker an
/// earlier hub left would.
fn said(build: Build) -> String {
    #[cfg(feature = "testing")]
    if let Ok(build) = std::env::var(TEST_BUILD) {
        return build;
    }
    build.version().to_string()
}

/// The variable that names the build a worker says it is, in tests.
#[cfg(feature = "testing")]
pub const TEST_BUILD: &str = "ROUNDHOUSE_TEST_WORKER_BUILD";
