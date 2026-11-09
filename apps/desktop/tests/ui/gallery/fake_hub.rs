//! A hub that answers from a fixed [`World`], so a screen looks the same on every run. It speaks the real protocol
//! over a real socket, so the app is unchanged, and it refuses anything that would change what it holds.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use protocol::PROTOCOL_VERSION;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubStatus, ReplyOutcome,
};
use protocol::frame;
use protocol::model::{GitDiff, GitStatus, SessionId, SessionStatus, WorkspaceStatus};

use super::world::World;

/// The build the app and its hub both say they are, so the label on every screen stays the same whatever this
/// checkout's version is.
pub(crate) const BUILD: &str = "1.0.0";
/// The app's build number, which a screen's hub can be older than.
pub(crate) const BUILD_NUMBER: u64 = 2;

/// Listens until the process ends. Each screen gets its own socket, so nothing outlives the screen that needs it.
pub(crate) fn serve(socket: &Path, world: World) {
    let listener = UnixListener::bind(socket).unwrap();
    let world = Arc::new(world);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let world = world.clone();
            std::thread::spawn(move || serve_connection(&world, stream));
        }
    });
}

/// Answers each request in turn. A subscription's events follow its reply straight away, and nothing comes
/// later, because nothing here changes.
fn serve_connection(world: &World, stream: UnixStream) {
    let mut writer = stream.try_clone().unwrap();
    for line in BufReader::new(stream).split(b'\n') {
        let Ok(line) = line else { return };
        let Ok(ClientFrame { id, request }) = frame::decode::<ClientFrame>(&line) else { return };
        let (outcome, then) = match answer(world, request) {
            Ok((response, then)) => (ReplyOutcome::Response(response), then),
            Err(error) => (ReplyOutcome::Error(error), Vec::new()),
        };
        for frame in std::iter::once(HubFrame::Reply { id, outcome }).chain(then) {
            if writer.write_all(&frame::encode(&frame).unwrap()).is_err() {
                return;
            }
        }
    }
}

/// The reply, and any frames that go after it.
type Answer = Result<(ClientResponse, Vec<HubFrame>), ApiError>;

#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
fn answer(world: &World, request: ClientRequest) -> Answer {
    use ClientRequest as R;
    use ClientResponse as C;
    let reply = |response| Ok((response, Vec::new()));
    match request {
        R::Hello { .. } => {
            reply(C::Hello { protocol_version: PROTOCOL_VERSION, hub_version: version(world).to_string() })
        }
        R::HubStatus => reply(C::HubStatus(status(world))),
        R::RepoList => reply(C::Repos { repos: world.repos.clone() }),
        R::RecordsList => reply(C::Records(world.records())),
        R::WorkspaceList { include_archived } => reply(C::Workspaces {
            workspaces: world
                .workspaces
                .iter()
                .filter(|w| include_archived || w.status != WorkspaceStatus::Archived)
                .cloned()
                .collect(),
        }),
        R::WorkspaceStatus { .. } => reply(C::GitStatus(GitStatus::default())),
        R::WorkspaceDiff { id } => reply(C::GitDiff(world.diffs.get(&id).cloned().unwrap_or_else(GitDiff::default))),
        R::WorkspaceSetupLog { id } => {
            reply(C::SetupLog { text: world.setup_logs.get(&id).cloned().unwrap_or_default() })
        }
        R::WorkspacePullRequest { id } => reply(C::PullRequest { pull_request: world.pull_requests.get(&id).cloned() }),
        R::CommentList { workspace_id } => {
            reply(C::Comments { comments: world.comments.get(&workspace_id).cloned().unwrap_or_default() })
        }
        R::SessionList { workspace_id } => reply(C::Sessions {
            sessions: world.sessions.iter().filter(|s| s.workspace_id == workspace_id).cloned().collect(),
        }),
        R::SessionGet { id } => reply(C::Session(world.session(&id)?.clone())),
        R::SessionEnsure { workspace_id, .. } => {
            let open = world.sessions.iter().rev().find(|s| s.workspace_id == workspace_id && s.closed_at_ms.is_none());
            reply(C::Session(open.cloned().ok_or_else(|| read_only("give this workspace a chat in the screen"))?))
        }
        R::SessionRead { id, from_seq, limit, .. } => {
            let session = world.session(&id)?;
            let events = world.events_from(&id, from_seq).take(usize::try_from(limit).unwrap_or(usize::MAX));
            reply(C::Events { events: events.cloned().collect(), last_seq: session.last_seq })
        }
        R::SessionSubscribe { id, from_seq, subscription_id } => subscribe(world, &id, from_seq, subscription_id),
        R::FeedSubscribe { .. } | R::SessionUnsubscribe { .. } => reply(C::Ok),
        R::HubShutdown
        | R::RepoAdd { .. }
        | R::WorkspaceCreate { .. }
        | R::WorkspaceArchive { .. }
        | R::WorkspaceRestore { .. }
        | R::WorkspaceRename { .. }
        | R::WorkspaceRerunSetup { .. }
        | R::CommentAdd { .. }
        | R::CommentDelete { .. }
        | R::SessionCreate { .. }
        | R::SessionPrompt { .. }
        | R::SessionApprove { .. }
        | R::SessionCancel { .. }
        | R::SessionConfigure { .. }
        | R::SessionClose { .. }
        | R::SessionReopen { .. }
        | R::SessionMarkRead { .. }
        | R::SessionMarkUnread { .. }
        | R::SessionRename { .. } => Err(read_only("change anything")),
        R::Unknown => Err(ApiError::new(ErrorCode::Unsupported, "the gallery's hub doesn't know that request")),
    }
}

/// The log from `from_seq`, then the streaming tail, as a real hub sends them once it has caught up.
fn subscribe(world: &World, id: &SessionId, from_seq: u64, subscription_id: u64) -> Answer {
    world.session(id)?;
    let events = world.events_from(id, from_seq).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = HubFrame::Tail { subscription_id, tail: world.tails.get(id).cloned().unwrap_or_default() };
    Ok((ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
}

fn version(world: &World) -> &'static str {
    world.hub.map_or(BUILD, |hub| hub.version)
}

/// Started by hand, as far as the app can tell, unless the screen says otherwise. A hub a client started is never
/// replaced, because its running chats keep it busy.
fn status(world: &World) -> HubStatus {
    let running = world
        .sessions
        .iter()
        .filter(|s| matches!(s.status, SessionStatus::Running | SessionStatus::WaitingApproval))
        .count();
    HubStatus {
        pid: std::process::id(),
        hub_version: version(world).to_string(),
        protocol_version: PROTOCOL_VERSION,
        started_at_ms: 0,
        workers: 0,
        running_sessions: u32::try_from(running).unwrap(),
        clients: 1,
        on_demand: world.hub.is_some_and(|hub| hub.on_demand),
        build_number: world.hub.map(|hub| hub.number),
        running_setups: 0,
        requests: 1,
    }
}

fn read_only(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("the gallery's hub only shows a fixed screen, so it can't {what}"))
}
