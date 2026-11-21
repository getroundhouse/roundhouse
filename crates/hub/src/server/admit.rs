//! What a connection has to do before anything else, and what one from the network may never ask. A device
//! authenticates first, then every client says hello.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, Hello, ReplyOutcome};
use protocol::frame;
use protocol::model::ClientDevice;
use protocol::pairing::device_name_from;
use tokio::sync::{OwnedSemaphorePermit, watch};
use tokio::time::Instant;
use tracing::warn;

use super::counted::Counted;
use crate::hub::Hub;
use crate::remote::{DeviceLink, Revoke};

/// Where a connection comes from, which decides what it has to do first and what it may ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
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
    /// From the network, before pairing or authenticating, which it has to do by `deadline`. Until then it holds a
    /// place among the connections that haven't authenticated.
    Authenticating { deadline: Instant, _unproven: OwnedSemaphorePermit },
    /// Counted among the hub's clients, which only a client on the hub's machine, or a device that has
    /// authenticated, is, so a connection from anyone on the network can't make the count say more than who's using
    /// it. Saying hello doesn't change the stage, since moving the count to another would need a third stage to leave
    /// behind meanwhile.
    Admitted { greeted: bool, _counted: Counted },
}

pub(super) struct Gate {
    origin: Origin,
    stage: Stage,
    /// The device a connection from the network authenticated as.
    pub(super) device: Option<DeviceLink>,
    /// The device a client on the hub's machine said in its hello it's on, if it said one the hub can show.
    said: Option<ClientDevice>,
    /// Set if the device the connection authenticated as is revoked.
    revoke: Revoke,
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
            stage: Stage::Admitted { greeted: false, _counted: Counted::client(hub) },
            device: None,
            said: None,
            revoke: Arc::new(watch::Sender::new(false)),
        }
    }

    /// A connection from the network, accepted [`AUTHENTICATE_WITHIN`] before `deadline`, holding `unproven` until it
    /// authenticates.
    pub(super) fn network(deadline: Instant, unproven: OwnedSemaphorePermit) -> Gate {
        Gate {
            origin: Origin::Network,
            stage: Stage::Authenticating { deadline, _unproven: unproven },
            device: None,
            said: None,
            revoke: Arc::new(watch::Sender::new(false)),
        }
    }

    /// The device the connection's client is on: the one it authenticated as, from the network, and otherwise the one
    /// it said in its hello, if any.
    pub(super) fn client_device(&self) -> Option<ClientDevice> {
        match &self.device {
            Some(link) => Some(ClientDevice { id: link.device_id.clone(), name: link.device_name.clone() }),
            None => self.said.clone(),
        }
    }

    /// Becomes true if the device the connection authenticated as is revoked.
    pub(super) fn revoked(&self) -> watch::Receiver<bool> {
        self.revoke.subscribe()
    }

    pub(super) fn max_frame_bytes(&self) -> usize {
        match self.stage {
            Stage::Authenticating { .. } => MAX_FIRST_FRAME_BYTES,
            Stage::Admitted { .. } => frame::MAX_FRAME_BYTES,
        }
    }

    /// When the next request has to have arrived by, if there's a limit.
    pub(super) fn deadline(&self) -> Option<Instant> {
        match self.stage {
            Stage::Authenticating { deadline, .. } => Some(deadline),
            Stage::Admitted { .. } => None,
        }
    }

    pub(super) async fn admit(&mut self, hub: &Arc<Hub>, request: ClientRequest) -> Admit {
        let origin = self.origin;
        match &mut self.stage {
            Stage::Authenticating { .. } => self.authenticate(hub, &request).await,
            Stage::Admitted { greeted: false, .. } if origin == Origin::Local && works_without_hello(&request) => {
                Admit::Handle(request)
            }
            Stage::Admitted { greeted: greeted @ false, .. } => match hello(hub, &request) {
                Ok(response) => {
                    *greeted = true;
                    // Over the network, the device is the one the connection authenticated as, whatever it says.
                    if origin == Origin::Local {
                        self.said = said_device(&request);
                    }
                    Admit::Answer(ReplyOutcome::Response(response))
                }
                Err(error) => Admit::Refuse(ReplyOutcome::Error(error)),
            },
            Stage::Admitted { greeted: true, .. } if origin == Origin::Network && local_only(&request) => {
                let message = "only a client on the hub's own machine can ask that";
                Admit::Answer(ReplyOutcome::Error(ApiError::new(ErrorCode::LocalOnly, message)))
            }
            Stage::Admitted { greeted: true, .. } => Admit::Handle(request),
        }
    }

    async fn authenticate(&mut self, hub: &Arc<Hub>, request: &ClientRequest) -> Admit {
        let authenticated = if let ClientRequest::Pair(protocol::client::devices::Pair { code, device_name }) = request
        {
            hub.pair(code, device_name, &self.revoke).await.map(|(token, link)| {
                (
                    ClientResponse::Paired(protocol::client::devices::Paired {
                        device_id: link.device_id.clone(),
                        token,
                    }),
                    link,
                )
            })
        } else if let ClientRequest::Authenticate(protocol::client::devices::Authenticate { token }) = request {
            hub.authenticate(token, &self.revoke).await.map(|link| (ClientResponse::Ok, link))
        } else {
            let message = "a connection from the network has to pair or authenticate first";
            Err(ApiError::new(ErrorCode::Unauthorized, message))
        };
        match authenticated {
            Ok((response, link)) => {
                self.device = Some(link);
                // Gives up its place among those that haven't authenticated.
                self.stage = Stage::Admitted { greeted: false, _counted: Counted::client(hub) };
                Admit::Answer(ReplyOutcome::Response(response))
            }
            Err(error) => Admit::Refuse(ReplyOutcome::Error(error)),
        }
    }
}

/// The device a hello says its client is on, if it says one with an ID and a name the hub can show, by the rules a
/// paired device's name follows. A hello isn't refused for it, since a hello only finds a version both sides speak, so
/// one the hub can't show is as good as none, and a request that needs a device says so.
fn said_device(request: &ClientRequest) -> Option<ClientDevice> {
    let ClientRequest::Hello(Hello { device: Some(device), .. }) = request else { return None };
    match device_name_from(&device.name) {
        Ok(name) if !device.id.as_str().trim().is_empty() => {
            Some(ClientDevice { id: device.id.clone(), name: name.to_string() })
        }
        Ok(_) => {
            warn!("a client said it's on a device with no ID, so it's taken to be on none");
            None
        }
        Err(e) => {
            warn!("a client said it's on a device whose name can't be shown, so it's taken to be on none: {e}");
            None
        }
    }
}

/// What a client of any version can do, so that a hub it can't otherwise talk to can still be seen and stopped.
/// Anyone who can reach the socket is this user. Only the socket gets this, because a device on the network may never
/// stop the hub.
fn works_without_hello(request: &ClientRequest) -> bool {
    matches!(request, ClientRequest::HubStatus | ClientRequest::HubShutdown | ClientRequest::HubRestart)
}

/// What only a client on the hub's own machine may ask: stopping the hub, and choosing which devices can reach
/// it. Every request is listed, so a new one has to be decided here.
#[expect(
    clippy::too_many_lines,
    reason = "every request is classified explicitly so new requests cannot accidentally expose local administration"
)]
fn local_only(request: &ClientRequest) -> bool {
    use ClientRequest as R;
    match request {
        R::PairingStart
        | R::PairingWait(..)
        | R::DeviceList
        | R::DeviceRevoke(..)
        | R::HubStatus
        | R::HubShutdown
        | R::HubShutdownIfIdle
        | R::HubRestart => true,
        // A hub installed as a service is used from other machines, so any paired device can update it, which
        // restarts it, as `docs/spec/hubs.md` says.
        R::HubUpdateTo(..)
        | R::Pair(..)
        | R::Authenticate(..)
        | R::Hello(..)
        | R::HubConfigure(..)
        | R::RepoAdd(..)
        | R::RepoList
        | R::RecordsList
        | R::ModelsFor(_)
        | R::AgentList
        | R::WorkspaceCreate(..)
        | R::WorkspaceList(..)
        | R::WorkspaceArchive(..)
        | R::WorkspaceRestore(..)
        | R::WorkspaceRename(..)
        | R::WorkspaceRegenerateTitle(..)
        | R::WorkspaceStatus(..)
        | R::WorkspaceDiff(..)
        | R::WorkspaceSetupLog(..)
        | R::WorkspaceRerunSetup(..)
        | R::WorkspacePullRequest(..)
        | R::ReferencesRead(..)
        | R::CommentAdd(..)
        | R::CommentList(..)
        | R::CommentEdit(..)
        | R::CommentDelete(..)
        | R::SessionCreate(..)
        | R::SessionEnsure(..)
        | R::SessionList(..)
        | R::SessionGet(..)
        | R::SessionRead(..)
        | R::SessionPromptList(..)
        | R::SessionCompactionList(..)
        | R::SessionFind(..)
        | R::SessionSubscribe(..)
        | R::SessionSubagents(..)
        | R::SessionProposals(..)
        | R::SessionRepliesHeard(..)
        | R::ProposalAnswer(..)
        | R::ProposalAnotherRound(..)
        | R::ProposalRevisionsRead(..)
        | R::SessionReadPlaces(..)
        | R::SessionSubscribePlaces(..)
        | R::SessionUnsubscribe(..)
        | R::FeedSubscribe(..)
        | R::GithubSubscribe(..)
        | R::SessionPrompt(..)
        | R::AttachmentAdd(..)
        | R::AttachmentRead(..)
        | R::AttachmentRemove(..)
        | R::SessionAccount(..)
        | R::SessionAssignAccount(..)
        | R::AccountConnect(..)
        | R::AccountSetDefault(..)
        | R::AccountSetAutomatic(..)
        | R::AccountDisconnect(..)
        | R::AccountSetReserve(..)
        | R::GithubAccountConnect(..)
        | R::GithubAccountConnectGh
        | R::HubGhLogin
        | R::HubMachineLogins
        | R::GithubAccountDisconnect(..)
        | R::GithubAccountSetDefault(..)
        | R::GithubSignInStart(..)
        | R::GithubSignInCancel(..)
        | R::GithubAppManifestWrite(..)
        | R::GithubAppMake(..)
        | R::GithubAppInstallationsConnect(..)
        | R::GithubAppRemove(..)
        | R::WorkspaceFiles(..)
        | R::WorkspaceFolderRead(..)
        | R::WorkspaceFileRead(..)
        | R::WorkspaceFileReadAt(..)
        | R::WorkspaceFileOpenAt(..)
        | R::WorkspaceFilePages(..)
        | R::SessionQueue(..)
        | R::SessionChangeQueued(..)
        | R::SessionUnqueue(..)
        | R::SessionSendQueued(..)
        | R::SessionDecide(..)
        | R::SessionCancel(..)
        | R::SessionCompact(..)
        | R::SessionConfigure(..)
        | R::SessionClose(..)
        | R::SessionReopen(..)
        | R::SessionMarkRead(..)
        | R::SessionMarkUnread(..)
        | R::SessionConversationStart(..)
        | R::SessionConversationEnd(..)
        | R::SessionReplyHeard(..)
        | R::SessionSpeaking(..)
        | R::SessionCallOffWait(..)
        | R::SessionDraft(..)
        | R::SessionDecisionDraft(..)
        | R::WorkspaceCommentDraft(..)
        | R::NewWorkspaceDraftKeep(..)
        | R::NewWorkspaceDraftFileAdd(..)
        | R::NewWorkspaceDraftFileRead(..)
        | R::NewWorkspaceDraftFileRemove(..)
        | R::NewWorkspaceCreate(..)
        | R::SessionRename(..)
        | R::TerminalOpen(..)
        | R::TerminalAttach(..)
        | R::TerminalWatch(..)
        | R::TerminalRows(..)
        | R::TerminalInput(..)
        | R::TerminalResize(..)
        | R::TerminalClose(..)
        | R::Unknown => false,
    }
}

fn hello(hub: &Hub, request: &ClientRequest) -> Result<ClientResponse, ApiError> {
    match super::hello::hello(hub.build(), Some(request)).0 {
        ReplyOutcome::Response(reply) => Ok(reply),
        ReplyOutcome::Error(error) => Err(error),
    }
}
