//! The hub's side of the provider protocol. Runs a provider's adapter as a child of the hub, and asks it what
//! [`Provider`] asks, so the rest of the hub treats it as it does any provider. Every request is safe to repeat,
//! so one whose adapter exits before answering is asked again once, of a new one.

use std::io;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use protocol::client::error_chain;
use protocol::frame;
use protocol::model::{HubId, InstanceId};
use protocol::provider::{
    PROVIDER_PROTOCOL_VERSION, ProviderCall, ProviderEvent, ProviderFrame, ProviderOutcome, ProviderRequest,
    ProviderResponse,
};
use providers::{Capabilities, Instance, Provider, ProviderError, Size, WorkerBinary, WorkerCommand};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tracing::{info, warn};

use crate::calls::{Calls, NotSent};
use crate::child_process::{self, ChildProcess, Spawned, Stderr};
use crate::frames::FrameReader;

/// An adapter answers hello at once, since it needn't reach its provider to do so.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// Far longer than a request should take, even a cloud's VM starting, so an adapter that has lost one can't keep the
/// hub waiting for it forever.
const REQUEST_TIMEOUT: Duration = Duration::from_mins(10);
/// How long an adapter gets to exit once its stdin closes.
pub(crate) const ADAPTER_GRACE: Duration = Duration::from_secs(5);

/// The command that runs an adapter.
#[derive(Clone, Debug)]
pub(crate) struct AdapterCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// An adapter's stdin and stdout, and its process, which an adapter in a test has none of.
pub(crate) struct Opened {
    pub reader: Box<dyn AsyncRead + Send + Unpin>,
    pub writer: Box<dyn AsyncWrite + Send + Unpin>,
    pub process: Option<ChildProcess>,
}

/// Starts the adapter. A process for real, and something in-process in tests.
pub(crate) type Open = Box<dyn Fn() -> io::Result<Opened> + Send + Sync>;

pub(crate) struct AdapterProvider {
    name: String,
    hub_id: HubId,
    open: Open,
    /// As the newest hello declared them.
    capabilities: Mutex<Capabilities>,
    connection: AsyncMutex<Option<Arc<Connection>>>,
    /// Set once the hub is shutting down, which gives up on a hello in flight and starts no more adapters.
    shut: watch::Sender<bool>,
}

impl AdapterProvider {
    /// Its adapter starts when it's first asked something, or told to say hello.
    pub(crate) fn new(name: String, hub_id: HubId, open: Open) -> Self {
        Self {
            name,
            hub_id,
            open,
            capabilities: Mutex::new(Capabilities::default()),
            connection: AsyncMutex::new(None),
            shut: watch::channel(false).0,
        }
    }

    /// Starts its adapter, unless it's running, and says hello, so a problem with it is found before anything needs
    /// it.
    pub(crate) async fn say_hello(&self) -> Result<(), ProviderError> {
        self.connection().await.map(drop)
    }

    /// Closes its adapter's stdin, and ends it if it hasn't exited within `grace`. Nothing starts it again.
    pub(crate) async fn shut_down(&self, grace: Duration) {
        self.shut.send_replace(true);
        let connection = self.connection.lock().await.take();
        if let Some(connection) = connection {
            connection.end(grace).await;
        }
    }

    /// The one running, or a new one if it has exited.
    async fn connection(&self) -> Result<Arc<Connection>, ProviderError> {
        let mut current = self.connection.lock().await;
        if let Some(connection) = current.as_ref().filter(|c| c.calls.is_open()) {
            return Ok(connection.clone());
        }
        let mut shut = self.shut.subscribe();
        let shutting_down = || ProviderError::io(format!("starting provider {}'s adapter", self.name), shut_down());
        if *shut.borrow() {
            return Err(shutting_down());
        }
        let opened =
            (self.open)().map_err(|e| ProviderError::io(format!("starting provider {}'s adapter", self.name), e))?;
        let connection = Connection::start(opened);
        let hello = ProviderRequest::Hello {
            protocol_version: PROVIDER_PROTOCOL_VERSION,
            oldest_protocol_version: PROVIDER_PROTOCOL_VERSION,
            hub_id: self.hub_id.clone(),
            build: protocol::BUILD_VERSION.to_string(),
        };
        let failed = |e: io::Error| ProviderError::io(format!("saying hello to provider {}", self.name), e);
        let answer = tokio::select! {
            answer = tokio::time::timeout(HELLO_TIMEOUT, connection.call(&hello)) => answer.map_err(|_| failed(timed_out()))?,
            _ = shut.wait_for(|shut| *shut) => return Err(shutting_down()),
        };
        match answer?.ok_or_else(|| failed(exited()))? {
            ProviderOutcome::Response(ProviderResponse::Hello { adapter, build, capabilities, .. }) => {
                info!(provider = self.name, adapter, build, "a provider's adapter said hello");
                *self.capabilities.lock() = capabilities;
            }
            ProviderOutcome::Response(other) => return Err(unexpected(&other)),
            // Not with the adapter's own code, which would read as the answer to whatever needed the hello.
            ProviderOutcome::Error(e) => return Err(failed(io::Error::other(e.message))),
        }
        *current = Some(connection.clone());
        Ok(connection)
    }

    async fn call(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let failed = |e: io::Error| ProviderError::io(format!("asking provider {}", self.name), e);
        let mut asked_again = false;
        loop {
            let connection = self.connection().await?;
            let answer = tokio::time::timeout(REQUEST_TIMEOUT, connection.call(&request))
                .await
                .map_err(|_| failed(timed_out()))?;
            match answer? {
                Some(ProviderOutcome::Response(response)) => return Ok(response),
                Some(ProviderOutcome::Error(e)) => return Err(ProviderError::Adapter(e)),
                None if !asked_again => {
                    warn!(provider = self.name, "a provider's adapter exited before answering, so asking a new one");
                    asked_again = true;
                }
                None => return Err(failed(exited())),
            }
        }
    }

    async fn call_ok(&self, request: ProviderRequest) -> Result<(), ProviderError> {
        match self.call(request).await? {
            ProviderResponse::Ok => Ok(()),
            other @ (ProviderResponse::Hello { .. }
            | ProviderResponse::Instances { .. }
            | ProviderResponse::Command(_)) => Err(unexpected(&other)),
        }
    }
}

fn exited() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "its adapter exited")
}

fn shut_down() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "the hub is shutting down")
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "its adapter didn't answer")
}

fn unexpected(response: &ProviderResponse) -> ProviderError {
    ProviderError::io("a provider's adapter", io::Error::other(format!("unexpected answer {response:?}")))
}

/// Runs `command` as the adapter. What it writes to stderr goes to the hub's.
pub(crate) fn process(command: AdapterCommand) -> Open {
    Box::new(move || {
        let Spawned { process, reader, writer } =
            child_process::spawn(&command.program, &command.args, &[], Stderr::Hubs)?;
        Ok(Opened { reader, writer, process: Some(process) })
    })
}

struct Connection {
    calls: Calls<ProviderOutcome>,
    /// Taken once it's being ended.
    process: Mutex<Option<ChildProcess>>,
}

impl Connection {
    fn start(opened: Opened) -> Arc<Self> {
        let exited = opened.process.as_ref().map(ChildProcess::exited);
        let connection = Arc::new(Self { calls: Calls::new(opened.writer), process: Mutex::new(opened.process) });
        tokio::spawn(read_frames(Arc::downgrade(&connection), opened.reader));
        // Its stdout can outlive it, held by something it started, so its exit ends the connection too.
        if let Some(exited) = exited {
            let connection = Arc::downgrade(&connection);
            tokio::spawn(async move {
                exited.await;
                if let Some(connection) = connection.upgrade().filter(|c| c.calls.is_open()) {
                    warn!("a provider's adapter exited");
                    connection.calls.end();
                }
            });
        }
        connection
    }

    /// The adapter's answer, or `None` if it has gone without giving one.
    async fn call(&self, request: &ProviderRequest) -> Result<Option<ProviderOutcome>, ProviderError> {
        let replied = match self.calls.send(|id| ProviderCall { id, request: request.clone() }).await {
            Ok(replied) => replied,
            Err(NotSent::Closed) => return Ok(None),
            Err(e @ NotSent::Encoding(_)) => return Err(ProviderError::io("asking an adapter", io::Error::other(e))),
        };
        Ok(replied.await.ok())
    }

    /// Closes its stdin, and kills it if it hasn't exited within `grace`.
    async fn end(&self, grace: Duration) {
        self.calls.end();
        let process = self.process.lock().take();
        if let Some(process) = process {
            process.end(grace).await;
        }
    }
}

impl Drop for Connection {
    /// As [`Connection::end`] does, with nobody waiting for it.
    fn drop(&mut self) {
        let Some(process) = self.process.get_mut().take() else { return };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn(async move { process.end(ADAPTER_GRACE).await })),
            Err(_) => process.kill(),
        }
    }
}

async fn read_frames(connection: Weak<Connection>, reader: Box<dyn AsyncRead + Send + Unpin>) {
    let mut incoming = FrameReader::new(reader);
    loop {
        let read = incoming.next().await;
        let Some(connection) = connection.upgrade() else { return };
        let line = match read {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(e) => {
                warn!("reading from a provider's adapter failed: {}", error_chain(&e));
                break;
            }
        };
        match frame::decode::<ProviderFrame>(line) {
            Ok(ProviderFrame::Reply { id, outcome }) => connection.calls.reply(id, outcome),
            Ok(ProviderFrame::Event { event }) => heard(&event),
            Err(e) => warn!("ignoring a frame from a provider's adapter that didn't parse: {e}"),
        }
    }
    if let Some(connection) = connection.upgrade() {
        connection.calls.end();
    }
}

/// *Not built:* acting on these, such as by telling the workspace's sessions their instance is going.
fn heard(event: &ProviderEvent) {
    match event {
        ProviderEvent::InstanceEnding { instance_id, reason, .. } => {
            warn!(%instance_id, reason, "an instance is ending");
        }
        ProviderEvent::InstanceLost { instance_id, reason } => warn!(%instance_id, reason, "an instance was lost"),
        ProviderEvent::Unknown => {}
    }
}

#[async_trait]
impl Provider for AdapterProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.lock().clone()
    }

    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        size: Option<Size>,
        display_name: &str,
    ) -> Result<(), ProviderError> {
        let (instance_id, from, display_name) = (instance_id.clone(), from.cloned(), display_name.to_string());
        self.call_ok(ProviderRequest::Create { instance_id, from, size, display_name }).await
    }

    async fn start(&self, instance_id: &InstanceId, workers: &[WorkerBinary]) -> Result<(), ProviderError> {
        self.call_ok(ProviderRequest::Start { instance_id: instance_id.clone(), workers: workers.to_vec() }).await
    }

    async fn stop(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.call_ok(ProviderRequest::Stop { instance_id: instance_id.clone() }).await
    }

    async fn kill(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.call_ok(ProviderRequest::Kill { instance_id: instance_id.clone() }).await
    }

    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.call_ok(ProviderRequest::Destroy { instance_id: instance_id.clone() }).await
    }

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        match self.call(ProviderRequest::List).await? {
            ProviderResponse::Instances { instances } => Ok(instances),
            other @ (ProviderResponse::Hello { .. } | ProviderResponse::Ok | ProviderResponse::Command(_)) => {
                Err(unexpected(&other))
            }
        }
    }

    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        match self.call(ProviderRequest::Connect { instance_id: instance_id.clone() }).await? {
            ProviderResponse::Command(command) => Ok(command),
            other @ (ProviderResponse::Hello { .. } | ProviderResponse::Ok | ProviderResponse::Instances { .. }) => {
                Err(unexpected(&other))
            }
        }
    }

    async fn make_base_image(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.call_ok(ProviderRequest::MakeBaseImage { instance_id: instance_id.clone() }).await
    }
}

#[cfg(test)]
mod tests;
