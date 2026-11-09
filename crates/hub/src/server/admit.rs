//! What a connection has to do before anything else, and what one from the network may never ask. A device
//! authenticates first, then every client says hello.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::frame;
use tokio::sync::{OwnedSemaphorePermit, watch};
use tokio::time::Instant;

use crate::hub::Hub;
use crate::remote::{DeviceLink, Revoke};

/// Where a connection comes from, which decides what it has to do first and what it may ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The Unix socket, which only this user can open, as `run::bind_socket` makes sure.
    Local,
    /// TLS over TCP, from anything that can reach the port.
    Network,
}

/// Far more than pairing or authenticating needs, and far less than the usual limit, so a connection that hasn't
/// authenticated can't make the hub hold much.
const MAX_FIRST_FRAME_BYTES: usize = 16 * 1024;
/// How long a connection from the network gets, from when it's accepted, to finish its TLS handshake and pair or
/// authenticate, so one that never does can't stay open. One deadline covers both, so a connection can't take the
/// whole of each.
pub(super) const AUTHENTICATE_WITHIN: Duration = Duration::from_secs(10);

enum Stage {
    /// From the network, before pairing or authenticating.
    Authenticating,
    Greeting,
    Open,
}

pub(super) struct Gate {
    origin: Origin,
    stage: Stage,
    /// The device a connection from the network authenticated as.
    pub(super) device: Option<DeviceLink>,
    /// When a connection from the network has to have authenticated by.
    deadline: Option<Instant>,
    /// A connection from the network's place among those that haven't authenticated yet, given up once it has.
    unproven: Option<OwnedSemaphorePermit>,
    /// Counts the connection among the hub's clients, which only a client on the hub's machine, or a device that
    /// has authenticated, is. The app replaces an older hub only while it's the one client, so a connection from
    /// anyone on the network could otherwise hold that up.
    counted: Option<Counted>,
    /// Set if the device the connection authenticated as is revoked.
    revoke: Revoke,
}

/// Counts a connection among the hub's clients for as long as it's held.
struct Counted(Arc<Hub>);

impl Counted {
    fn new(hub: &Arc<Hub>) -> Counted {
        hub.clients.fetch_add(1, Ordering::Relaxed);
        Counted(hub.clone())
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.clients.fetch_sub(1, Ordering::Relaxed);
    }
}

/// What to do with a request.
pub(super) enum Admit {
    /// Answer it as usual, alongside whatever comes after it.
    Handle(ClientRequest),
    /// Answered here, before anything after it is read.
    Answer(ReplyOutcome),
    /// Answered here, and then the connection closes.
    Refuse(ReplyOutcome),
}

impl Gate {
    /// A connection on the Unix socket, counted among the hub's clients from the start.
    pub(super) fn local(hub: &Arc<Hub>) -> Gate {
        Gate {
            origin: Origin::Local,
            stage: Stage::Greeting,
            device: None,
            deadline: None,
            unproven: None,
            counted: Some(Counted::new(hub)),
            revoke: Arc::new(watch::Sender::new(false)),
        }
    }

    /// A connection from the network, accepted [`AUTHENTICATE_WITHIN`] before `deadline`, holding `unproven` until it
    /// authenticates.
    pub(super) fn network(deadline: Instant, unproven: OwnedSemaphorePermit) -> Gate {
        Gate {
            origin: Origin::Network,
            stage: Stage::Authenticating,
            device: None,
            deadline: Some(deadline),
            unproven: Some(unproven),
            counted: None,
            revoke: Arc::new(watch::Sender::new(false)),
        }
    }

    /// Becomes true if the device the connection authenticated as is revoked.
    pub(super) fn revoked(&self) -> watch::Receiver<bool> {
        self.revoke.subscribe()
    }

    pub(super) fn max_frame_bytes(&self) -> usize {
        match self.stage {
            Stage::Authenticating => MAX_FIRST_FRAME_BYTES,
            Stage::Greeting | Stage::Open => frame::MAX_FRAME_BYTES,
        }
    }

    /// When the next request has to have arrived by, if there's a limit.
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub(super) async fn admit(&mut self, hub: &Arc<Hub>, request: ClientRequest) -> Admit {
        match self.stage {
            Stage::Authenticating => self.authenticate(hub, &request).await,
            Stage::Greeting if self.origin == Origin::Local && works_without_hello(&request) => Admit::Handle(request),
            Stage::Greeting => match hello(&request) {
                Ok(response) => {
                    self.stage = Stage::Open;
                    Admit::Answer(ReplyOutcome::Response(response))
                }
                Err(error) => Admit::Refuse(ReplyOutcome::Error(error)),
            },
            Stage::Open if self.origin == Origin::Network && local_only(&request) => {
                let message = "only a client on the hub's own machine can ask that";
                Admit::Answer(ReplyOutcome::Error(ApiError::new(ErrorCode::LocalOnly, message)))
            }
            Stage::Open => Admit::Handle(request),
        }
    }

    async fn authenticate(&mut self, hub: &Arc<Hub>, request: &ClientRequest) -> Admit {
        let authenticated = if let ClientRequest::Pair { code, device_name } = request {
            hub.pair(code, device_name, &self.revoke)
                .await
                .map(|(token, link)| (ClientResponse::Paired { device_id: link.device_id.clone(), token }, link))
        } else if let ClientRequest::Authenticate { token } = request {
            hub.authenticate(token, &self.revoke).await.map(|link| (ClientResponse::Ok, link))
        } else {
            let message = "a connection from the network has to pair or authenticate first";
            Err(ApiError::new(ErrorCode::Unauthorized, message))
        };
        match authenticated {
            Ok((response, link)) => {
                self.device = Some(link);
                self.stage = Stage::Greeting;
                self.deadline = None;
                self.unproven = None;
                self.counted = Some(Counted::new(hub));
                Admit::Answer(ReplyOutcome::Response(response))
            }
            Err(error) => Admit::Refuse(ReplyOutcome::Error(error)),
        }
    }
}

/// What a client of any version can do, so that a hub it can't otherwise talk to can still be seen and stopped.
/// Anyone who can reach the socket is this user. Only the socket gets this, because stopping a shared hub is for its
/// admins.
fn works_without_hello(request: &ClientRequest) -> bool {
    matches!(request, ClientRequest::HubStatus | ClientRequest::HubShutdown)
}

/// What only a client on the hub's own machine may ask: stopping the hub, and choosing which devices can reach
/// it. Every request is listed, so a new one has to be decided here.
fn local_only(request: &ClientRequest) -> bool {
    use ClientRequest as R;
    match request {
        R::HubStatus
        | R::HubShutdown
        | R::PairingStart
        | R::PairingWait { .. }
        | R::DeviceList
        | R::DeviceRevoke { .. } => true,
        R::Hello { .. }
        | R::Pair { .. }
        | R::Authenticate { .. }
        | R::RepoAdd { .. }
        | R::RepoList
        | R::RecordsList
        | R::WorkspaceCreate { .. }
        | R::WorkspaceList { .. }
        | R::WorkspaceArchive { .. }
        | R::WorkspaceRestore { .. }
        | R::WorkspaceStatus { .. }
        | R::WorkspaceDiff { .. }
        | R::WorkspaceSetupLog { .. }
        | R::WorkspaceRerunSetup { .. }
        | R::WorkspacePullRequest { .. }
        | R::CommentAdd { .. }
        | R::CommentList { .. }
        | R::CommentDelete { .. }
        | R::SessionCreate { .. }
        | R::SessionEnsure { .. }
        | R::SessionList { .. }
        | R::SessionGet { .. }
        | R::SessionRead { .. }
        | R::SessionSubscribe { .. }
        | R::SessionUnsubscribe { .. }
        | R::FeedSubscribe { .. }
        | R::SessionPrompt { .. }
        | R::SessionApprove { .. }
        | R::SessionCancel { .. }
        | R::SessionConfigure { .. }
        | R::SessionClose { .. }
        | R::SessionReopen { .. }
        | R::Unknown => false,
    }
}

fn hello(request: &ClientRequest) -> Result<ClientResponse, ApiError> {
    let error = |message: String| ApiError::new(ErrorCode::VersionMismatch, message);
    let ClientRequest::Hello { protocol_version, .. } = request else {
        return Err(error("the first request on a connection must be hello".to_string()));
    };
    if *protocol_version == PROTOCOL_VERSION {
        Ok(ClientResponse::Hello {
            protocol_version: PROTOCOL_VERSION,
            hub_version: protocol::BUILD_VERSION.to_string(),
        })
    } else {
        Err(error(format!(
            "this hub is version {} on protocol {PROTOCOL_VERSION}, and this client speaks protocol {protocol_version}. Stopping the hub, which also stops its agents, lets one that matches start",
            protocol::BUILD_VERSION
        )))
    }
}
