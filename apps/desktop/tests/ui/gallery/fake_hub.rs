//! A hub that answers from a fixed [`World`], so a screen looks the same on every run. It speaks the real protocol
//! over a real socket, so the app is unchanged, and it refuses anything that would change what it holds.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use protocol::attachment::AttachmentBytes;
use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubStatus, ReplyOutcome, drafts, files,
};
use protocol::event::SessionEvent;
use protocol::find::{Found, Needle};
use protocol::frame;
use protocol::model::{
    AgentDescription, AgentFeature, AgentKind, CompactionLine, Effort, FileContents, FileOpened, FilePage, FilePages,
    GitDiff, GitStatus, ModelChoice, ModelName, OpenedText, PageLine, PermissionMode, Place, PromptLine, Rev,
    SessionId, SessionSettings, SessionStatus, ToolUseId, WorkspaceStatus,
};

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
            reply(C::Hello { protocol_version: CLIENT_PROTOCOL_VERSION, hub_version: version(world).to_string() })
        }
        R::HubStatus => reply(C::HubStatus(status(world))),
        R::RepoList => reply(C::Repos { repos: world.repos.clone() }),
        R::RecordsList => reply(C::Records(world.records())),
        // Even as an older hub, so a screen of one draws the agent as the app's own hub would describe it.
        R::AgentList => reply(C::Agents { agents: vec![claude()] }),
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
        // A draft is taken as a hub that keeps it would, though it's left out of the screen's records, which don't
        // change. So is an answer being typed.
        R::SessionGet { id }
        | R::SessionDraft(drafts::SessionDraft { id, .. })
        | R::SessionAnswerDraft(drafts::SessionAnswerDraft { id, .. }) => {
            reply(C::Session(world.session(&id)?.clone()))
        }
        // As a hub too old to keep it, so a comment being written is what restarting would lose.
        R::WorkspaceCommentDraft(_) => {
            Err(ApiError::new(ErrorCode::Unsupported, "the gallery's hub doesn't keep a comment being written"))
        }
        // The same, for what's in the new-workspace dialog.
        R::NewWorkspaceDraftKeep(_)
        | R::NewWorkspaceDraftFileAdd(_)
        | R::NewWorkspaceDraftFileRead(_)
        | R::NewWorkspaceDraftFileRemove(_) => Err(ApiError::new(
            ErrorCode::Unsupported,
            "the gallery's hub doesn't keep what's in the new-workspace dialog",
        )),
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
            let prompts = world.events_from(&id, 1).filter_map(PromptLine::of);
            reply(C::Prompts { prompts: prompts.collect() })
        }
        // An older hub's build is from before compactions were listed too.
        R::SessionCompactionList { .. } if world.hub.is_some() => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't list compactions"))
        }
        R::SessionCompactionList { id } => {
            world.session(&id)?;
            let compactions = world.events_from(&id, 1).filter_map(CompactionLine::of);
            reply(C::Compactions { compactions: compactions.collect() })
        }
        R::SessionFind { id, subagent, query } => {
            world.session(&id)?;
            let needle = Needle::new(&query).map_err(|e| ApiError::new(ErrorCode::InvalidRequest, e.to_string()))?;
            let theirs: Vec<&SessionEvent> = world.placed_from(&id, subagent.as_ref(), Place::new(1)).collect();
            let places = theirs.iter().filter(|event| needle.is_in_event(&event.body)).map(|event| event.place);
            let through = theirs.last().map_or(Place::new(0), |event| event.place);
            reply(C::Found(Found { places: places.collect(), through }))
        }
        R::SessionSubscribe { id, from_seq, subscription_id } => subscribe(world, &id, from_seq, subscription_id),
        R::SessionReadPlaces { id, subagent, from_place, limit, .. } => {
            world.session(&id)?;
            let last_place =
                world.placed_from(&id, subagent.as_ref(), Place::new(1)).last().map_or(Place::new(0), |e| e.place);
            let events = world
                .placed_from(&id, subagent.as_ref(), from_place)
                .take(usize::try_from(limit).unwrap_or(usize::MAX));
            reply(C::PlacedEvents { events: events.cloned().collect(), last_place })
        }
        R::SessionSubscribePlaces { id, subagent, from_place, subscription_id } => {
            subscribe_places(world, &id, subagent.as_ref(), from_place, subscription_id)
        }
        R::SessionSubagents { id } => {
            world.session(&id)?;
            reply(C::Subagents { subagents: world.subagents_of(&id), rev: Rev::default() })
        }
        R::FeedSubscribe { .. } | R::SessionUnsubscribe { .. } => reply(C::Ok),
        R::AttachmentRead { id, .. } => {
            let data = world.attachments.get(&id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such file"))?;
            reply(C::AttachmentData { data: AttachmentBytes::new(data.clone()) })
        }
        // An older hub's build is from before files were listed, too.
        R::WorkspaceFiles(_) if world.hub.is_some() => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't list files"))
        }
        R::WorkspaceFiles(_) => reply(C::WorktreeFiles(world.worktree.clone())),
        R::WorkspaceFolderRead(files::WorkspaceFolderRead { path, .. }) => {
            let files = world
                .worktree_folders
                .get(&path)
                .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such folder"))?;
            reply(C::WorktreeFiles(files.clone()))
        }
        R::WorkspaceFileRead(files::WorkspaceFileRead { path, .. }) => {
            let file =
                world.worktree_files.get(&path).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such file"))?;
            reply(C::FileContents(file.clone()))
        }
        R::WorkspaceFileReadAt(files::WorkspaceFileReadAt { path, .. }) => {
            let file =
                world.base_files.get(&path).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "not in the base"))?;
            reply(C::FileContents(file.clone()))
        }
        R::WorkspaceFileOpenAt(files::WorkspaceFileOpenAt { path, .. }) => {
            let text = base_text(world, &path)?;
            let lines = u32::try_from(text.lines().count()).unwrap();
            let pages = pages_of(text, PAGE_LINES, &(0..lines.div_ceil(PAGE_LINES)).collect::<Vec<_>>());
            let bytes = u64::try_from(text.len()).unwrap();
            reply(C::FileOpened(FileOpened::Text(OpenedText { bytes, lines, page_lines: PAGE_LINES, pages })))
        }
        R::WorkspaceFilePages(files::WorkspaceFilePages { path, page_lines, pages, .. }) => {
            reply(C::FilePages(FilePages { pages: pages_of(base_text(world, &path)?, page_lines, &pages) }))
        }
        R::HubShutdown
        | R::HubRestart
        | R::HubShutdownIfIdle
        | R::HubConfigure { .. }
        | R::RepoAdd { .. }
        | R::WorkspaceCreate { .. }
        | R::WorkspaceArchive { .. }
        | R::WorkspaceRestore { .. }
        | R::WorkspaceRename { .. }
        | R::WorkspaceRerunSetup { .. }
        | R::CommentAdd { .. }
        | R::CommentEdit { .. }
        | R::CommentDelete { .. }
        | R::SessionCreate { .. }
        | R::SessionPrompt { .. }
        | R::AttachmentAdd { .. }
        | R::AttachmentRemove { .. }
        | R::SessionQueue(_)
        | R::SessionChangeQueued(_)
        | R::SessionUnqueue(_)
        | R::SessionSendQueued(_)
        | R::SessionApprove { .. }
        | R::SessionCancel { .. }
        | R::SessionCompact { .. }
        | R::SessionConfigure { .. }
        | R::SessionClose { .. }
        | R::SessionReopen { .. }
        | R::SessionCallOffWait(_)
        | R::SessionMarkRead { .. }
        | R::SessionMarkUnread { .. }
        | R::SessionRename { .. } => Err(read_only("change anything")),
        // So a screen can show a workspace waiting for its new title. One on its way out is the world's to say.
        R::WorkspaceRegenerateTitle { .. } => Ok(Outcome::Never),
        // The app reads whose account it is from the records, so a screen that asks some other way is wrong.
        R::SessionAccount { .. } | R::Unknown => {
            Err(ApiError::new(ErrorCode::Unsupported, "the gallery's hub doesn't know that request"))
        }
    }
}

/// The log from `from_seq`, then the streaming tail, as a real hub sends them once it has caught up.
fn subscribe(world: &World, id: &SessionId, from_seq: u64, subscription_id: u64) -> Answer {
    world.session(id)?;
    let events = world.events_from(id, from_seq).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = HubFrame::Tail { subscription_id, tail: world.tails.get(id).cloned().unwrap_or_default() };
    Ok(Outcome::Reply(ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
}

/// The same, for the agent's own events or one subagent's, with only that one's part of what's still streaming.
fn subscribe_places(
    world: &World,
    id: &SessionId,
    subagent: Option<&ToolUseId>,
    from: Place,
    subscription_id: u64,
) -> Answer {
    world.session(id)?;
    let events =
        world.placed_from(id, subagent, from).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = world.tails.get(id).map(|tail| tail.of(subagent)).unwrap_or_default();
    let tail = HubFrame::Tail { subscription_id, tail };
    Ok(Outcome::Reply(ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
}

/// Claude as the app's own hub describes it, so the picker, the context's cards and a turn's card say what they would.
fn claude() -> AgentDescription {
    let offered =
        |model: &'static str, label: &str| ModelChoice { model: ModelName::named(model), label: label.into() };
    AgentDescription {
        kind: AgentKind::Claude,
        name: "Claude Code".into(),
        usage_heading: "Claude usage".into(),
        models: vec![
            offered("fable", "Fable"),
            offered("opus", "Opus"),
            offered("sonnet", "Sonnet"),
            offered("haiku", "Haiku"),
        ],
        efforts: Effort::ALL.to_vec(),
        modes: PermissionMode::CHOSEN.to_vec(),
        defaults: SessionSettings {
            model: ModelName::named("opus"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::Xhigh,
        },
        features: vec![AgentFeature::Compacts],
    }
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
        protocol_version: CLIENT_PROTOCOL_VERSION,
        started_at_ms: 0,
        workers: 0,
        running_sessions: u32::try_from(running).unwrap(),
        clients: 1,
        on_demand: world.hub.is_some_and(|hub| hub.on_demand),
        build_number: world.hub.map(|hub| hub.number),
        running_setups: 0,
        requests: 1,
        keeping_awake: false,
        keeps_workers: false,
        oldest_protocol_version: None,
    }
}

fn read_only(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("the gallery's hub only shows a fixed screen, so it can't {what}"))
}

/// How many lines a page of a base file has, as the worker pages them.
const PAGE_LINES: u32 = 256;

/// The text of a world's base file at `path`.
fn base_text<'a>(world: &'a World, path: &str) -> Result<&'a str, ApiError> {
    match world.base_files.get(path) {
        Some(FileContents::Text { text }) => Ok(text),
        _ => Err(ApiError::new(ErrorCode::NotFound, "no text in the base")),
    }
}

/// Pages `pages` of `text`, with `page_lines` lines a page, as the worker sends them.
fn pages_of(text: &str, page_lines: u32, pages: &[u32]) -> Vec<FilePage> {
    let (lines, size): (Vec<&str>, usize) = (text.lines().collect(), usize::try_from(page_lines).unwrap());
    let page = |index: u32| {
        let first = usize::try_from(index).unwrap() * size;
        let lines = lines.get(first..(first + size).min(lines.len()))?;
        let lines = lines.iter().map(|line| PageLine { text: (*line).to_string(), cut: None }).collect();
        Some(FilePage { index, lines })
    };
    pages.iter().filter_map(|index| page(*index)).collect()
}
