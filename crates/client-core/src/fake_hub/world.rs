//! Everything the fake hub serves, and the builders that make its repositories, workspaces and terminals. Records get
//! IDs and times in the order they're made, so a test gets the same data on every run. A world's chats are made as
//! `super::chats` says, and what GitHub says of it as `super::github` does.

use std::collections::HashMap;

use protocol::client::ApiError;
use protocol::event::SessionEvent;
use protocol::model::{
    Account, AttachmentId, DiffComment, FileContents, GitDiff, GithubAccount, GithubApp, GithubSettings, GithubSignIn,
    GithubSignInStatus, InstanceId, PORTS_PER_WORKSPACE, Proposal, ReplyHeard, Repo, RepoId, RepoOrigin, Rev, Session,
    SessionId, Subagent, Terminal, TerminalId, Workspace, WorkspaceId, WorktreeFiles,
};
use protocol::tail::Tail;
use protocol::terminal::TerminalSize;

/// When the first record was made: 1 November 2026, at 2 PM in UTC, which pictures show times in. Each one after it is
/// a minute later, and each event a second.
const START_MS: i64 = 1_793_541_600_000;
const RECORD_GAP_MS: i64 = 60_000;
pub(super) const EVENT_GAP_MS: i64 = 1_000;
pub(super) const MINUTE_MS: i64 = 60_000;
pub const HOUR_MS: i64 = 60 * MINUTE_MS;
/// What the app takes as the time now, in every picture: an hour after the first record, so later than any record a
/// world makes, and on the same day, so a turn that ended says only its time.
pub const NOW_MS: i64 = START_MS + 60 * MINUTE_MS;

#[derive(Default)]
pub struct World {
    /// As [`World::offer_models`] makes them.
    pub model_lists: Vec<protocol::model::ModelList>,
    pub agents: Option<Vec<protocol::model::AgentDescription>>,
    pub repos: Vec<Repo>,
    pub workspaces: Vec<Workspace>,
    pub sessions: Vec<Session>,
    /// Each session's events, with `seq` from 1 and no gaps, as the protocol promises, and each its place among the
    /// events from the same agent or subagent.
    pub(super) logs: HashMap<SessionId, Vec<SessionEvent>>,
    /// Each session's subagents, made from its events as [`Subagent::recorded_from`] says, so they agree with them.
    pub(super) subagents: HashMap<SessionId, Vec<Subagent>>,
    /// Each session's proposals, in the order they were made.
    pub proposals: HashMap<SessionId, Vec<Proposal>>,
    /// What's still streaming, for a chat caught mid-reply, and how far a reply being read out loud has got.
    pub tails: HashMap<SessionId, Tail>,
    /// How each chat's replies read out loud ended, as the hub records them.
    pub replies_heard: HashMap<SessionId, Vec<ReplyHeard>>,
    pub diffs: HashMap<WorkspaceId, GitDiff>,
    pub comments: HashMap<WorkspaceId, Vec<DiffComment>>,
    pub setup_logs: HashMap<WorkspaceId, String>,
    /// What GitHub says of the world's workspaces and of what its chats link to, and the form the hub sends it in.
    pub github_says: super::github::GithubSays,
    /// What clients follow now, which a test shares before the hub is given the world, to tell what a client follows
    /// and when.
    pub following: super::Following,
    /// What clients asked, which a test shares in the same way, to tell what a client read.
    pub asked: super::Asked,
    /// The bytes of the files the hub keeps, by attachment: the files prompts carry, the images tools send back, and
    /// the images and videos the agent's text points to.
    pub attachments: HashMap<AttachmentId, Vec<u8>>,
    /// The files and folders in the world's worktree, as All files lists them.
    pub worktree: WorktreeFiles,
    /// What's in each folder of the world's worktree that's read when it's opened, by its path.
    pub worktree_folders: HashMap<String, WorktreeFiles>,
    /// What the files in the world's worktree that can be opened hold, by path.
    pub worktree_files: HashMap<String, FileContents>,
    /// The bytes of the videos in the world's worktree, which a player reads a part at a time, by path.
    pub worktree_videos: HashMap<String, Vec<u8>>,
    /// What the files the world's diff changes held in the commit it was taken from, by path.
    pub base_files: HashMap<String, FileContents>,
    /// The hub's build, for a screen about a hub that isn't the app's build.
    pub hub: Option<HubBuild>,
    /// The records of the accounts the world's chats run under.
    pub accounts: Vec<Account>,
    pub account_defaults: Vec<protocol::model::AccountDefault>,
    /// What the hub's settings say of GitHub, or nothing, for a world whose screens don't show it.
    pub github: Option<GithubSettings>,
    pub github_accounts: Vec<GithubAccount>,
    pub github_sign_ins: Vec<GithubSignIn>,
    /// Who `gh` on the hub's machine is logged in as, as the hub answers each time it's asked. `None` fails to say, as
    /// a hub that couldn't read `gh` does.
    pub hub_gh_login: Option<protocol::model::GhLogin>,
    /// Who `gh` and the SSH key on the hub's machine are, as the hub answers each time it's asked. `None` fails to say,
    /// as a hub that couldn't read them does.
    pub machine_logins: Option<protocol::model::MachineLogins>,
    /// Why the hub couldn't use `gh`'s login as an account's, as it answers each time it's asked. `None` refuses it, as the
    /// rest of what would change what the hub holds.
    pub gh_connect_refused: Option<ApiError>,
    /// How a sign-in with GitHub the app starts stands as the hub answers, which it does with the ID the app gave it,
    /// though it keeps no record of it. `None` refuses it, as the rest of what would change what the hub holds.
    pub sign_in_answer: Option<GithubSignInStatus>,
    /// The apps of your own the hub has made.
    pub github_apps: Vec<GithubApp>,
    /// Why the hub couldn't look for where an app of your own is installed, as it answers each time it's asked. `None`
    /// refuses it, as the rest of what would change what the hub holds.
    pub app_look_refused: Option<ApiError>,
    /// Whether `pairing_start` hands out a deterministic pairing link, and whether `pairing_wait` resolves with this
    /// device name or stays waiting.
    pub pairing: Option<PairingWorld>,
    /// Saved remote hubs written to `settings.json` before the window opens.
    pub saved_hubs: Vec<(&'static str, &'static str)>,
    /// Each open terminal, and what it shows.
    pub terminals: Vec<(Terminal, TerminalShows)>,
    /// What the hub found in the update feed, as a hub installed as a service does. `None` as from one that doesn't
    /// read it.
    pub hub_update: Option<protocol::model::HubUpdate>,
    /// The observations the hub keeps, of its level, listed with its records, and of a workspace's, to its followers.
    pub observations: Vec<protocol::model::Observation>,
    /// What each workspace's worker reads of its changes on request, as [`WorkerReads`] says.
    pub worker_reads: HashMap<WorkspaceId, super::WorkerReads>,
    /// What the feed tells of observations once a client follows their level, in order: what changed after the list.
    pub feed: Vec<protocol::client::ObservationNotice>,
    /// The hub's newest revision, which its lists are at, as [`World::rev`] finds it once the world is served.
    pub(super) listed_at: Rev,
    records: i64,
}

#[derive(Clone, Copy)]
pub struct PairingWorld {
    pub paired_device: Option<&'static str>,
}

/// A hub's build, other than the app's.
#[derive(Clone, Copy)]
pub struct HubBuild {
    pub version: &'static str,
    pub number: u64,
    /// A client started it, rather than someone running it themselves.
    pub on_demand: bool,
}

/// What a fake hub's terminal shows: the bytes that draw it in a fresh terminal of its size, which a worker's copy of
/// it would have taken in.
#[derive(Clone, Debug)]
pub struct TerminalShows {
    pub size: TerminalSize,
    pub bytes: Vec<u8>,
}

impl World {
    pub fn repo(&mut self, name: &str) -> RepoId {
        let id = RepoId::from(format!("repo-{}", self.repos.len() + 1));
        let created_at_ms = self.next_time();
        self.repos.push(Repo {
            id: id.clone(),
            name: name.to_string(),
            origin: RepoOrigin::Checkout { path: format!("/src/{name}") },
            created_at_ms,
            rev: Rev::default(),
        });
        id
    }

    /// Ready, with setup done. Change what a test needs through [`World::workspace_mut`].
    pub fn workspace(&mut self, repo: &RepoId, name: &str) -> WorkspaceId {
        let id = WorkspaceId::from(format!("ws-{}", self.workspaces.len() + 1));
        let created_at_ms = self.next_time();
        let port_base = 4000 + PORTS_PER_WORKSPACE * u16::try_from(self.workspaces.len()).unwrap();
        self.workspaces.push(Workspace {
            repo_id: repo.clone(),
            name: name.to_string(),
            branch: name.into(),
            base_ref: Some("origin/main".to_string()),
            path: Some(format!("/workspaces/{name}")),
            instance_id: InstanceId::from("local"),
            created_at_ms,
            user_active_at_ms: Some(created_at_ms),
            port_base,
            ..protocol::fixtures::workspace(id.as_str())
        });
        id
    }

    /// A terminal open in `workspace`, showing `shows`, at `size`.
    pub fn terminal(&mut self, workspace: &WorkspaceId, size: TerminalSize, shows: &[u8]) -> TerminalId {
        let id = TerminalId::from(format!("term-{}", self.terminals.len() + 1));
        let opened_at_ms = self.next_time();
        let terminal = Terminal { id: id.clone(), workspace_id: workspace.clone(), opened_at_ms, rev: Rev::default() };
        self.terminals.push((terminal, TerminalShows { size, bytes: shows.to_vec() }));
        id
    }

    pub fn workspace_mut(&mut self, id: &WorkspaceId) -> &mut Workspace {
        self.workspaces.iter_mut().find(|w| &w.id == id).unwrap()
    }

    pub(super) fn next_time(&mut self) -> i64 {
        self.records += 1;
        START_MS + RECORD_GAP_MS * self.records
    }
}
