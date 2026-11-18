use std::collections::HashMap;
use std::convert::Infallible;
use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use protocol::attachment::AttachmentBytes;
use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::client::log::ReadLimit;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HelloReply, HubFrame, ReplyOutcome, admin,
    attachments, comments, drafts, files, github_accounts, github_apps, log, records, sessions, subscriptions,
    workspaces,
};
use protocol::event::SessionEvent;
use protocol::find::{Found, Needle};
use protocol::frame;
use protocol::model::{
    CompactionLine, GitDiff, GitStatus, GithubSignIn, Place, PromptLine, Rev, Seq, SessionId, SettledAt, SubagentId,
    WorkspaceStatus,
};

use super::base_pages::{opened, paged};
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

pub(super) type Answer = Result<Outcome, ApiError>;

#[expect(clippy::large_enum_variant, reason = "one is made for each request and sent straight away, never kept")]
pub(super) enum Outcome {
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
        R::Hello(_) => reply(C::Hello(HelloReply {
            protocol_version: CLIENT_PROTOCOL_VERSION,
            hub_version: version(world).to_string(),
        })),
        R::HubStatus => reply(C::HubStatus(status(world))),
        R::RepoList => reply(C::Repos(records::Repos { repos: world.repos.clone() })),
        R::RecordsList => reply(C::Records(world.records())),
        R::ModelsFor(asked) => reply(C::ModelList(world.model_list_for(&asked)?)),
        // As the app's own hub describes its agent, even as an older hub, so a screen of one draws the agent as it
        // would, but for what makes a login, which a hub from before hubs kept them doesn't say.
        R::AgentList => {
            reply(C::Agents(admin::Agents { agents: world.agents.clone().unwrap_or_else(|| vec![claude(world)]) }))
        }
        R::WorkspaceList(workspaces::WorkspaceList { include_archived }) => {
            reply(C::Workspaces(workspaces::Workspaces {
                workspaces: world
                    .workspaces
                    .iter()
                    .filter(|w| include_archived || w.status != WorkspaceStatus::Archived)
                    .cloned()
                    .collect(),
            }))
        }
        R::WorkspaceStatus(_) => reply(C::GitStatus(GitStatus::default())),
        R::WorkspaceDiff(workspaces::WorkspaceDiff { id }) => {
            reply(C::GitDiff(world.diffs.get(&id).cloned().unwrap_or_else(GitDiff::default)))
        }
        R::WorkspaceSetupLog(workspaces::WorkspaceSetupLog { id }) => {
            reply(C::SetupLog(workspaces::SetupLog { text: world.setup_logs.get(&id).cloned().unwrap_or_default() }))
        }
        R::WorkspacePullRequest(workspaces::WorkspacePullRequest { id }) => {
            reply(C::PullRequest(workspaces::PullRequestReply { pull_request: world.pull_requests.get(&id).cloned() }))
        }
        R::CommentList(comments::CommentList { workspace_id }) => reply(C::Comments(comments::Comments {
            comments: world.comments.get(&workspace_id).cloned().unwrap_or_default(),
        })),
        R::SessionList(sessions::SessionList { workspace_id }) => reply(C::Sessions(sessions::Sessions {
            sessions: world.sessions.iter().filter(|s| s.workspace_id == workspace_id).cloned().collect(),
        })),
        // A draft is taken as a hub that keeps it would, though it's left out of the world's records, which don't
        // change. So is an answer being typed.
        R::SessionGet(sessions::SessionGet { id })
        | R::SessionDraft(drafts::SessionDraft { id, .. })
        | R::SessionAnswerDraft(drafts::SessionAnswerDraft { id, .. }) => {
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
            let open = world.sessions.iter().rev().find(|s| s.workspace_id == workspace_id && s.is_open());
            reply(C::Session(open.cloned().ok_or_else(|| read_only("give this workspace a chat"))?))
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
        R::SessionSubscribe(subscriptions::SessionSubscribe { id, from_seq, subscription_id }) => {
            subscribe(world, &id, from_seq, subscription_id)
        }
        R::SessionReadPlaces(_) | R::SessionSubscribePlaces(_) if world.reads_every_event_together => {
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
        R::SessionSubscribePlaces(subscriptions::SessionSubscribePlaces {
            id,
            subagent,
            from_place,
            subscription_id,
        }) => {
            let subagent = world.subagent_named(&id, subagent.as_ref());
            subscribe_places(world, &id, subagent.as_ref(), from_place, subscription_id)
        }
        R::SessionSubagents(records::SessionSubagents { id }) => {
            world.session(&id)?;
            reply(C::Subagents(records::Subagents { subagents: world.subagents_of(&id), rev: Rev::default() }))
        }
        R::FeedSubscribe(_) | R::SessionUnsubscribe(_) => reply(C::Ok),
        R::AttachmentRead(attachments::AttachmentRead { id, .. }) => {
            let data = world.attachments.get(&id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such file"))?;
            reply(C::AttachmentData(attachments::AttachmentData { data: AttachmentBytes::new(data.clone()) }))
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
        R::HubShutdown
        | R::HubRestart
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
        | R::AttachmentAdd(_)
        | R::AttachmentRemove(_)
        | R::SessionQueue(_)
        | R::SessionChangeQueued(_)
        | R::SessionUnqueue(_)
        | R::SessionSendQueued(_)
        | R::SessionApprove(_)
        | R::SessionCancel(_)
        | R::SessionCompact(_)
        | R::SessionConfigure(_)
        | R::SessionClose(_)
        | R::SessionReopen(_)
        | R::SessionCallOffWait(_)
        | R::SessionMarkRead(_)
        | R::SessionMarkUnread(_)
        | R::SessionRename(_)
        | R::AccountConnect(_)
        | R::AccountSetDefault(_)
        | R::SessionAssignAccount(_)
        | R::AccountDisconnect(_)
        | R::GithubAccountConnect(_)
        | R::GithubAccountDisconnect(_)
        | R::GithubAccountSetDefault(_)
        | R::GithubSignInCancel(_)
        | R::GithubAppMake(_)
        | R::GithubAppRemove(_) => Err(read_only("change anything")),
        R::GithubAppInstallationsConnect(_) => Err(world.app_look_refused.clone().unwrap_or_else(|| read_only("look"))),
        // As a hub writes it, which changes nothing the hub holds, though with only what the app reads of it, its name
        // and where GitHub sends the person back, since nothing here posts it to GitHub.
        R::GithubAppManifestWrite(github_apps::GithubAppManifestWrite { name, redirect_url }) => {
            let manifest = serde_json::json!({"name": name, "redirect_url": redirect_url, "public": false});
            reply(C::GithubAppManifest(github_apps::GithubAppManifest { manifest }))
        }
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
        | R::Unknown => Err(ApiError::new(ErrorCode::Unsupported, "a fake hub doesn't know that request")),
        R::TerminalAttach(attach) => super::terminals::attach(world, &attach),
        R::TerminalOpen(_) | R::TerminalInput(_) | R::TerminalResize(_) | R::TerminalClose(_) => {
            Err(read_only("change a terminal"))
        }
    }
}

/// The log from `from_seq`, then the streaming tail, as a real hub sends them once it has caught up.
fn subscribe(world: &World, id: &SessionId, from_seq: Seq, subscription_id: u64) -> Answer {
    world.session(id)?;
    let events = world.events_from(id, from_seq).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = HubFrame::Tail { subscription_id, tail: world.tails.get(id).cloned().unwrap_or_default() };
    Ok(Outcome::Reply(ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
}

/// The same, for the agent's own events or one subagent's, with only that one's part of what's still streaming.
fn subscribe_places(
    world: &World,
    id: &SessionId,
    subagent: Option<&SubagentId>,
    from: Place,
    subscription_id: u64,
) -> Answer {
    world.session(id)?;
    let events =
        world.placed_from(id, subagent, from).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = world.tails.get(id).map_or_default(|tail| tail.of(subagent));
    let tail = HubFrame::Tail { subscription_id, tail };
    Ok(Outcome::Reply(ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
}

/// What a read of `events` is sent, by what each takes in a hub's log: its body as JSON, and its raw line.
fn read<'a>(limit: ReadLimit, events: impl Iterator<Item = &'a SessionEvent>) -> Vec<SessionEvent> {
    let sized = events.map(|event| {
        let size = serde_json::to_string(&event.body).unwrap().len() + event.raw.as_ref().map_or(0, String::len);
        Ok::<_, Infallible>((event.clone(), u64::try_from(size).unwrap()))
    });
    let Ok(sent) = limit.cut(sized);
    sent
}

/// Each prompt in the chat as a line, oldest first, with where it was settled, which is the last event to settle it.
fn prompt_lines(world: &World, id: &SessionId) -> Vec<PromptLine> {
    let settled: HashMap<Seq, SettledAt> = world.events_from(id, Seq::new(1)).filter_map(SettledAt::of).collect();
    let line = |line: PromptLine| match settled.get(&line.seq()) {
        Some(at) => line.settled_at(*at),
        None => line,
    };
    world.events_from(id, Seq::new(1)).filter_map(PromptLine::of).map(line).collect()
}

fn read_only(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("a fake hub holds only what it was made with, so it can't {what}"))
}
