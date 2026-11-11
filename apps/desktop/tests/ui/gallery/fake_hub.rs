//! A hub that answers from a fixed [`World`], so a screen looks the same on every run. It speaks the real protocol
//! over a real socket, so the app is unchanged, and it refuses anything that would change what it holds.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use protocol::PROTOCOL_VERSION;
use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubStatus, ReplyOutcome,
};
use protocol::frame;
use protocol::model::{FileContents, GitDiff, GitStatus, PromptLine, SessionId, SessionStatus, WorkspaceStatus};

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

/// Answers each request in turn, but for one left unanswered. A subscription's events follow its reply straight
/// away, and nothing comes later, because nothing here changes.
fn serve_connection(world: &World, stream: UnixStream) {
    let mut writer = stream.try_clone().unwrap();
    for line in BufReader::new(stream).split(b'\n') {
        let Ok(line) = line else { return };
        let Ok(ClientFrame { id, request }) = frame::decode::<ClientFrame>(&line) else { return };
        let (outcome, then) = match answer(world, request) {
            Ok(Outcome::Reply(response, then)) => (ReplyOutcome::Response(response), then),
            Ok(Outcome::Never) => continue,
            Err(error) => (ReplyOutcome::Error(error), Vec::new()),
        };
        for frame in std::iter::once(HubFrame::Reply { id, outcome }).chain(then) {
            if writer.write_all(&frame::encode(&frame).unwrap()).is_err() {
                return;
            }
        }
    }
}

type Answer = Result<Outcome, ApiError>;

#[expect(clippy::large_enum_variant, reason = "one is made for each request and sent straight away, never kept")]
enum Outcome {
    /// The reply, and any frames that go after it.
    Reply(ClientResponse, Vec<HubFrame>),
    /// Left unanswered, so the app stays waiting, as for a screen that shows something under way.
    Never,
}

#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
fn answer(world: &World, request: ClientRequest) -> Answer {
    use ClientRequest as R;
    use ClientResponse as C;
    let reply = |response| Ok(Outcome::Reply(response, Vec::new()));
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
        // An older hub's build is from before prompts were listed.
        R::SessionPromptList { .. } if world.hub.is_some() => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't list prompts"))
        }
        R::SessionPromptList { id } => {
            world.session(&id)?;
            let prompts = world.events_from(&id, 1).filter_map(|event| PromptLine::of(event.seq, &event.body));
            reply(C::Prompts { prompts: prompts.collect() })
        }
        R::SessionSubscribe { id, from_seq, subscription_id } => subscribe(world, &id, from_seq, subscription_id),
        R::FeedSubscribe { .. } | R::SessionUnsubscribe { .. } => reply(C::Ok),
        R::AttachmentRead { id, .. } => {
            let data = world.attachments.get(&id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such file"))?;
            reply(C::AttachmentData { data: AttachmentBytes::new(data.clone()) })
        }
        R::WorkspaceImageRead { path, .. } => {
            let data =
                world.worktree_images.get(&path).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such image"))?;
            reply(C::Image { media_type: MediaType::sniff(data), data: AttachmentBytes::new(data.clone()) })
        }
        R::SessionAccount { .. } => {
            let unsupported =
                || ApiError::new(ErrorCode::Unsupported, "this screen's hub can't say whose account it is");
            reply(C::SessionAccount(world.account.clone().ok_or_else(unsupported)?))
        }
        // An older hub's build is from before files were listed, too.
        R::WorkspaceFiles { .. } if world.hub.is_some() => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't list files"))
        }
        R::WorkspaceFiles { .. } => reply(C::WorktreeFiles(world.worktree.clone())),
        R::WorkspaceFolderRead { path, .. } => {
            let files = world
                .worktree_folders
                .get(&path)
                .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such folder"))?;
            reply(C::WorktreeFiles(files.clone()))
        }
        R::WorkspaceFileRead { path, .. } => {
            let text =
                world.worktree_texts.get(&path).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such file"))?;
            reply(C::FileContents(FileContents::Text { text: text.clone() }))
        }
        R::HubShutdown
        | R::HubShutdownIfIdle
        | R::HubConfigure { .. }
        | R::RepoAdd { .. }
        | R::WorkspaceCreate { .. }
        | R::WorkspaceRestore { .. }
        | R::WorkspaceRename { .. }
        | R::WorkspaceRerunSetup { .. }
        | R::CommentAdd { .. }
        | R::CommentEdit { .. }
        | R::CommentDelete { .. }
        | R::SessionCreate { .. }
        | R::SessionPrompt { .. }
        | R::AttachmentAdd { .. }
        | R::SessionQueue { .. }
        | R::SessionUnqueue { .. }
        | R::SessionSendQueued { .. }
        | R::SessionApprove { .. }
        | R::SessionCancel { .. }
        | R::SessionConfigure { .. }
        | R::SessionClose { .. }
        | R::SessionReopen { .. }
        | R::SessionMarkRead { .. }
        | R::SessionMarkUnread { .. }
        | R::SessionRename { .. } => Err(read_only("change anything")),
        // So a screen can show a workspace on its way, or waiting for its new title.
        R::WorkspaceArchive { .. } | R::WorkspaceRegenerateTitle { .. } => Ok(Outcome::Never),
        R::Unknown => Err(ApiError::new(ErrorCode::Unsupported, "the gallery's hub doesn't know that request")),
    }
}

/// The log from `from_seq`, then the streaming tail, as a real hub sends them once it has caught up.
fn subscribe(world: &World, id: &SessionId, from_seq: u64, subscription_id: u64) -> Answer {
    world.session(id)?;
    let events = world.events_from(id, from_seq).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = HubFrame::Tail { subscription_id, tail: world.tails.get(id).cloned().unwrap_or_default() };
    Ok(Outcome::Reply(ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
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
        keeping_awake: false,
    }
}

fn read_only(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("the gallery's hub only shows a fixed screen, so it can't {what}"))
}
