use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use protocol::attachment::AttachmentBytes;
use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::log::ReadLimit;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HelloReply, HubFrame, ReplyOutcome, admin,
    attachments, comments, drafts, files, github_accounts, log, proposals, records, references, sessions, workspaces,
};
use protocol::event::SessionEvent;
use protocol::find::{Found, Needle};
use protocol::frame;
use protocol::model::{
    ChangedFilesPage, CompactionLine, GitDiff, GitStatus, GithubSignIn, MOST_FILES_LISTED, Place, Rev, Seq, Session,
};

use super::base_pages::{opened, paged};
use super::following::not_found;
use super::github;
use super::logs::{prompt_lines, read};
use super::parts::{part, worktree_video};
use super::stand_in::{claude, fixed_pairing, status, version};
use super::world::{LoginSupport, NOW_MS, World};

/// Listens until the process ends, since nothing stops it. Each world gets a socket of its own, so no test reaches
/// another's.
pub fn serve(socket: &Path, world: World) {
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
    let connection = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
    serve_requests(world, connection, stream);
    world.following.ended(connection);
}

/// The number the next connection to a fake hub is given, for what it follows.
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(0);

fn serve_requests(world: &World, connection: u64, stream: UnixStream) {
    let mut writer = stream.try_clone().unwrap();
    for line in BufReader::new(stream).split(b'\n') {
        let Ok(line) = line else { return };
        let Ok(ClientFrame { id, request }) = frame::decode::<ClientFrame>(&line) else { return };
        let (first, outcome, then) = match answer(world, connection, request) {
            Ok(Outcome::Reply { first, response, then }) => (first, ReplyOutcome::Response(response), then),
            Ok(Outcome::Never) => continue,
            Err(error) => (Vec::new(), ReplyOutcome::Error(error), Vec::new()),
        };
        for frame in first.into_iter().chain(std::iter::once(HubFrame::Reply { id, outcome })).chain(then) {
            if writer.write_all(&frame::encode(&frame).unwrap()).is_err() {
                return;
            }
        }
    }
}

pub(super) type Answer = Result<Outcome, ApiError>;

impl Outcome {
    /// The reply alone.
    pub(super) fn reply(response: ClientResponse) -> Self {
        Outcome::Reply { first: Vec::new(), response, then: Vec::new() }
    }
}

#[expect(clippy::large_enum_variant, reason = "one is made for each request and sent straight away, never kept")]
pub(super) enum Outcome {
    /// The reply, any frames a hub queues before it, as a workspace's list before the reply to `Subscribe`, and any
    /// that go after it.
    Reply { first: Vec<HubFrame>, response: ClientResponse, then: Vec<HubFrame> },
    /// Left unanswered, so the app stays waiting, as for a screen that shows something under way.
    Never,
}

#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
fn answer(world: &World, connection: u64, request: ClientRequest) -> Answer {
    use ClientRequest as R;
    use ClientResponse as C;
    let reply = |response| Ok(Outcome::reply(response));
    match request {
        R::Hello(_) => reply(C::Hello(HelloReply {
            protocol_version: CLIENT_PROTOCOL_VERSION,
            hub_version: version(world).to_string(),
        })),
        R::HubStatus => reply(C::HubStatus(status(world))),
        R::Subscribe(subscribe) => Ok(super::following::subscribe(world, connection, subscribe)),
        R::RepoList => reply(C::Repos(records::Repos { repos: world.repos_listed() })),
        R::ModelsFor(asked) => reply(C::ModelList(world.model_list_for(&asked)?)),
        // As the app's own hub describes its agent, even as an older hub, so a screen of one draws the agent as it
        // would, but for what makes a login, which a hub from before hubs kept them doesn't say.
        R::AgentList => {
            reply(C::Agents(admin::Agents { agents: world.agents.clone().unwrap_or_else(|| vec![claude(world)]) }))
        }
        R::WorkspaceList(workspaces::WorkspaceList { include_archived }) => {
            reply(C::Workspaces(workspaces::Workspaces { workspaces: world.workspaces_listed(include_archived) }))
        }
        R::WorkspaceStatus(_) => reply(C::GitStatus(GitStatus::default())),
        R::WorkspaceDiff(workspaces::WorkspaceDiff { id }) => {
            reply(C::GitDiff(world.diffs.get(&id).cloned().unwrap_or_else(GitDiff::default)))
        }
        R::WorkspaceChangedFiles(workspaces::WorkspaceChangedFiles { id, after }) => {
            world.workspaces.iter().find(|workspace| workspace.id == id).ok_or_else(|| not_found("workspace"))?;
            // As a hub whose worker is too old to read them.
            let Some((base, files)) = world.changed_files.get(&id) else {
                return Err(ApiError::new(ErrorCode::Unsupported, "the workspace's worker can't page through its files"));
            };
            reply(C::ChangedFilesPage(ChangedFilesPage::after(base.clone(), files.clone(), &after, MOST_FILES_LISTED)))
        }
        R::WorkspaceSetupLog(workspaces::WorkspaceSetupLog { id }) => {
            reply(C::SetupLog(workspaces::SetupLog { text: world.setup_logs.get(&id).cloned().unwrap_or_default() }))
        }
        R::WorkspacePullRequest(workspaces::WorkspacePullRequest { id }) => {
            reply(C::PullRequest(workspaces::PullRequestReply { pull_request: world.pull_requests.get(&id).cloned() }))
        }
        R::ReferencesRead(references::ReferencesRead { references, .. }) => {
            let references = references.iter().map(|ask| github::reference(world, ask)).collect();
            reply(C::References(references::References { references }))
        }
        R::CommentList(comments::CommentList { workspace_id }) => {
            reply(C::Comments(comments::Comments { comments: world.comments_listed(&workspace_id) }))
        }
        R::SessionList(sessions::SessionList { workspace_id }) => {
            reply(C::Sessions(sessions::Sessions { sessions: world.sessions_listed(&workspace_id) }))
        }
        // A draft is taken as a hub that keeps it would, though it's left out of the world's records, which don't
        // change. So is a decision being drafted.
        R::SessionGet(sessions::SessionGet { id })
        | R::SessionDraft(drafts::SessionDraft { id, .. })
        | R::SessionDecisionDraft(drafts::SessionDecisionDraft { id, .. }) => {
            reply(C::Session(world.session(&id)?.clone()))
        }
        // As an older hub refuses it. Agents asks none, since such a hub says nothing of what makes a login, so the
        // refusal itself is tested against a stand-in, with `Kept`.
        R::AccountConnect(_) if world.logins == LoginSupport::None => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't keep logins"))
        }
        // As a hub that started it would answer, as the world says it stands, though this one keeps no record of it, so
        // a screen can show how a sign-in the app started went. The app shows it from the record the answer carries.
        R::GithubSignInStart(github_accounts::GithubSignInStart { id }) => {
            let status = world.sign_in_answer.clone().ok_or_else(|| read_only("change anything"))?;
            reply(C::GithubSignIn(GithubSignIn { id, status, rev: Rev::default() }))
        }
        // As a hub from before it, since the world holds each workspace's diff whole, and nothing reads it from a fake
        // hub yet.
        R::WorkspaceFileDiff(_) => Err(ApiError::new(ErrorCode::Unsupported, "a fake hub doesn't read one file's diff")),
        // As a hub too old to keep it, so a comment being written is what restarting would lose.
        R::WorkspaceCommentDraft(_) => {
            Err(ApiError::new(ErrorCode::Unsupported, "a fake hub doesn't keep a comment being written"))
        }
        // The same, for what's in the new-workspace dialog.
        R::NewWorkspaceDraftKeep(_)
        | R::NewWorkspaceDraftFileAdd(_)
        | R::NewWorkspaceDraftFileRead(_)
        | R::NewWorkspaceDraftFileRemove(_)
        | R::NewWorkspaceCreate(_) => {
            Err(ApiError::new(ErrorCode::Unsupported, "a fake hub doesn't keep what's in the new-workspace dialog"))
        }
        R::SessionEnsure(sessions::SessionEnsure { workspace_id, .. }) => {
            let open = world.sessions_listed(&workspace_id).into_iter().rfind(Session::is_open);
            reply(C::Session(open.ok_or_else(|| read_only("give this workspace a chat"))?))
        }
        R::SessionRead(log::SessionRead { id, from_seq, limit, max_bytes, .. }) => {
            let session = world.session(&id)?;
            let events = read(ReadLimit::asked(limit, max_bytes), world.events_from(&id, from_seq));
            reply(C::Events(log::Events { events, last_seq: session.last_seq }))
        }
        // An older hub's build is from before prompts were listed.
        R::SessionPromptList(_) if world.hub.is_some() => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't list prompts"))
        }
        R::SessionPromptList(log::SessionPromptList { id }) => {
            world.session(&id)?;
            reply(C::Prompts(log::Prompts { prompts: prompt_lines(world, &id) }))
        }
        // An older hub's build is from before compactions were listed too.
        R::SessionCompactionList(_) if world.hub.is_some() => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't list compactions"))
        }
        R::SessionCompactionList(log::SessionCompactionList { id }) => {
            world.session(&id)?;
            let compactions = world.events_from(&id, Seq::new(1)).filter_map(CompactionLine::of);
            reply(C::Compactions(log::Compactions { compactions: compactions.collect() }))
        }
        R::SessionFind(log::SessionFind { id, subagent, query }) => {
            world.session(&id)?;
            let needle = Needle::new(&query).map_err(|e| ApiError::new(ErrorCode::InvalidRequest, e.to_string()))?;
            let subagent = world.subagent_named(&id, subagent.as_ref());
            let theirs: Vec<&SessionEvent> = world.placed_from(&id, subagent.as_ref(), Place::new(1)).collect();
            let places = theirs.iter().filter(|event| needle.is_in_event(&event.body)).map(|event| event.place);
            let through = theirs.last().map_or(Place::new(0), |event| event.place);
            reply(C::Found(Found { places: places.collect(), through }))
        }
        R::SessionReadPlaces(_) if world.reads_every_event_together => {
            Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't read the agent's own events apart"))
        }
        R::SessionReadPlaces(log::SessionReadPlaces { id, subagent, from_place, limit, max_bytes, .. }) => {
            world.session(&id)?;
            let subagent = world.subagent_named(&id, subagent.as_ref());
            let last_place =
                world.placed_from(&id, subagent.as_ref(), Place::new(1)).last().map_or(Place::new(0), |e| e.place);
            let events =
                read(ReadLimit::asked(limit, max_bytes), world.placed_from(&id, subagent.as_ref(), from_place));
            reply(C::PlacedEvents(log::PlacedEvents { events, last_place }))
        }
        // The world keeps only each proposal's newest revision, so it reads no earlier one.
        R::ProposalRevisionsRead(proposals::ProposalRevisionsRead { id, from, .. }) => {
            let proposal = world.proposals.values().flatten().find(|proposal| proposal.id == id);
            let current = &proposal.ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such proposal"))?.current;
            let revisions = match from.cmp(&current.number) {
                std::cmp::Ordering::Less => {
                    return Err(ApiError::new(ErrorCode::Unsupported, "the fake hub keeps only the newest revision"));
                }
                std::cmp::Ordering::Equal => vec![current.clone()],
                std::cmp::Ordering::Greater => Vec::new(),
            };
            reply(C::ProposalRevisions(proposals::ProposalRevisions { revisions, newest: current.number }))
        }
        R::AttachmentRead(attachments::AttachmentRead { id, .. }) => {
            let data = world.attachments.get(&id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such file"))?;
            reply(C::AttachmentData(attachments::AttachmentData { data: AttachmentBytes::new(data.clone()) }))
        }
        R::AttachmentReadRange(attachments::AttachmentReadRange { id, start, length, .. }) => {
            reply(C::FileRange(part(world.attachments.get(&id), start, length)?))
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
        R::WorkspaceFileOpenAt(files::WorkspaceFileOpenAt { path, .. }) => reply(C::FileOpened(opened(world, &path)?)),
        R::WorkspaceFilePages(files::WorkspaceFilePages { path, page_lines, pages, .. }) => {
            reply(C::FilePages(paged(world, &path, page_lines, &pages)?))
        }
        R::WorkspaceFileReadRange(files::WorkspaceFileReadRange { path, start, length, .. }) => {
            reply(C::FileRange(part(worktree_video(world, &path), start, length)?))
        }
        R::HubShutdown
        | R::HubRestart
        | R::HubUpdateTo(_)
        | R::HubShutdownIfIdle
        | R::HubConfigure(_)
        | R::RepoAdd(_)
        | R::WorkspaceArchive(_)
        | R::WorkspaceRestore(_)
        | R::WorkspaceRename(_)
        | R::WorkspaceRerunSetup(_)
        | R::CommentAdd(_)
        | R::CommentEdit(_)
        | R::CommentDelete(_)
        | R::SessionCreate(_)
        | R::SessionPrompt(_)
        | R::ProposalAnswer(_)
        | R::ProposalAnotherRound(_)
        | R::AttachmentAdd(_)
        | R::AttachmentRemove(_)
        | R::SessionQueue(_)
        | R::SessionChangeQueued(_)
        | R::SessionUnqueue(_)
        | R::SessionSendQueued(_)
        | R::SessionDecide(_)
        | R::SessionCancel(_)
        | R::SessionCompact(_)
        | R::SessionConfigure(_)
        | R::SessionClose(_)
        | R::SessionReopen(_)
        | R::SessionCallOffWait(_)
        | R::SessionMarkRead(_)
        | R::SessionMarkUnread(_)
        | R::SessionConversationStart(_)
        | R::SessionConversationEnd(_)
        | R::SessionReplyHeard(_)
        | R::SessionSpeaking(_)
        | R::SessionRename(_)
        | R::AccountConnect(_)
        | R::AccountSetDefault(_)
        | R::AccountSetAutomatic(_)
        | R::SessionAssignAccount(_)
        | R::AccountDisconnect(_)
        | R::AccountSetReserve(_)
        | R::GithubAccountConnect(_)
        | R::GithubAccountDisconnect(_)
        | R::GithubAccountSetDefault(_)
        | R::GithubSignInCancel(_)
        | R::GithubAppMake(_)
        | R::GithubAppRemove(_) => Err(read_only("change anything")),
        R::GithubAccountConnectGh => github::connect_gh(world),
        R::HubGhLogin => github::gh_login(world),
        R::HubMachineLogins => github::machine_logins(world),
        R::GithubAppInstallationsConnect(_) => github::look_for_installations(world),
        R::GithubAppManifestWrite(write) => reply(github::manifest(write)),
        // So a screen can show the app waiting on a workspace it asked for, or on a new title. The workspace's record
        // saying it's being made, or on its way out, is the world's to say.
        R::WorkspaceCreate(_) | R::WorkspaceRegenerateTitle(_) => Ok(Outcome::Never),
        R::PairingStart if world.pairing.is_some() => reply(C::Pairing(fixed_pairing())),
        R::PairingWait(_) if world.pairing.is_some() => match world.pairing.and_then(|p| p.paired_device) {
            Some(name) => reply(C::Device(protocol::pairing::Device {
                id: "dev-1".into(),
                name: name.to_string(),
                paired_at_ms: NOW_MS,
                last_seen_ms: NOW_MS,
                connections: 1,
            })),
            None => Ok(Outcome::Never),
        },
        // The app reads whose account it is from the records, so a screen that asks some other way is wrong.
        R::SessionAccount(_)
        | R::Pair(_)
        | R::Authenticate(_)
        | R::PairingStart
        | R::PairingWait(_)
        | R::DeviceList
        | R::DeviceRevoke(_)
        // What only a client of version 3 sends, which follows through a request of its own for each of what it follows,
        // and lists the records, as a client of this build doesn't.
        | R::RecordsList
        | R::FeedSubscribe(_)
        | R::SessionSubscribe(_)
        | R::SessionSubscribePlaces(_)
        | R::SessionUnsubscribe(_)
        | R::SessionSubagents(_)
        | R::SessionProposals(_)
        | R::SessionRepliesHeard(_)
        | R::GithubSubscribe(_)
        | R::TerminalWatch(_)
        | R::Unknown => Err(ApiError::new(ErrorCode::Unsupported, "a fake hub doesn't know that request")),
        R::TerminalRows(rows) => super::terminals::rows(world, &rows),
        R::TerminalBounds(bounds) => super::terminals::bounds(world, &bounds),
        R::TerminalText(text) => super::terminals::text(world, &text),
        R::TerminalOpen(_) | R::TerminalTyped(_) | R::TerminalFit(_) | R::TerminalClose(_) => {
            Err(read_only("change a terminal"))
        }
    }
}

pub(super) fn read_only(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("a fake hub holds only what it was made with, so it can't {what}"))
}
