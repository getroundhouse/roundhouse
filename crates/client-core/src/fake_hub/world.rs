//! Everything the fake hub serves, and the builders that make it. Records get IDs and times in the order they're
//! made, so a test gets the same data on every run.

use std::collections::HashMap;

use protocol::client::ApiError;
use protocol::client::ErrorCode;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{
    Account, AttachmentId, CacheTtl, ContextUsage, DiffComment, FileContents, GitDiff, GithubAccount, GithubApp,
    GithubSettings, GithubSignIn, GithubSignInStatus, InstanceId, ModelName, OwnPullRequest, PORTS_PER_WORKSPACE,
    Place, PromptCache, Proposal, ReplyHeard, Repo, RepoId, RepoOrigin, Rev, Seq, Session, SessionId, SessionStatus,
    Subagent, SubagentId, Terminal, TerminalId, Title, Workspace, WorkspaceId, WorktreeFiles,
};
use protocol::prompt::Prompt;
use protocol::tail::Tail;
use protocol::terminal::TerminalSize;

/// When the first record was made: 1 November 2026, at 2 PM in UTC, which pictures show times in. Each one after it is
/// a minute later, and each event a second.
const START_MS: i64 = 1_793_541_600_000;
const RECORD_GAP_MS: i64 = 60_000;
const EVENT_GAP_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60_000;
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
    logs: HashMap<SessionId, Vec<SessionEvent>>,
    /// Each session's subagents, made from its events as [`Subagent::recorded_from`] says, so they agree with them.
    subagents: HashMap<SessionId, Vec<Subagent>>,
    /// Each session's proposals, in the order they were made.
    pub proposals: HashMap<SessionId, Vec<Proposal>>,
    /// What's still streaming, for a chat caught mid-reply, and how far a reply being read out loud has got.
    pub tails: HashMap<SessionId, Tail>,
    /// How each chat's replies read out loud ended, as the hub records them.
    pub replies_heard: HashMap<SessionId, Vec<ReplyHeard>>,
    pub diffs: HashMap<WorkspaceId, GitDiff>,
    pub comments: HashMap<WorkspaceId, Vec<DiffComment>>,
    pub setup_logs: HashMap<WorkspaceId, String>,
    /// Each workspace's own pull request, as the hub would have read it.
    pub pull_requests: HashMap<WorkspaceId, OwnPullRequest>,
    /// The workspaces whose repository GitHub showed the default none of, as the hub last read it.
    pub unseen: HashMap<WorkspaceId, protocol::model::RepositoryUnseen>,
    /// The issues and pull requests on GitHub that chats link to, as the hub would have looked them up.
    pub references: Vec<protocol::model::Reference>,
    /// What clients follow now, which a test shares before the hub is given the world, to tell what a client follows
    /// and when.
    pub following: super::Following,
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
    /// Whether the hub is from before it read the agent's own events apart from its subagents', so a chat holds every
    /// event, and a subagent's work is gathered into its call.
    pub reads_every_event_together: bool,
    /// The hub's build, for a screen about a hub that isn't the app's build.
    pub hub: Option<HubBuild>,
    /// The records of the accounts the world's chats run under.
    pub accounts: Vec<Account>,
    pub account_defaults: Vec<protocol::model::AccountDefault>,
    /// The hub is from before hubs kept settings, so it sends none.
    pub keeps_no_settings: bool,
    pub logins: LoginSupport,
    /// What the hub's settings say of GitHub: `None` for a hub that keeps no GitHub accounts.
    pub github: Option<GithubSettings>,
    pub github_accounts: Vec<GithubAccount>,
    pub github_sign_ins: Vec<GithubSignIn>,
    /// Who `gh` on the hub's machine is logged in as, as the hub answers each time it's asked. `None` refuses it, as a hub
    /// from before it read `gh` does.
    pub hub_gh_login: Option<protocol::model::GhLogin>,
    /// Who `gh` and the SSH key on the hub's machine are, as the hub answers each time it's asked. `None` refuses it,
    /// as a hub from before it read them does.
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
    /// What a push from each repository's workspaces would meet on GitHub, as the hub last checked.
    pub github_reach: Vec<protocol::model::GithubReach>,
    /// Each open terminal, and what it shows.
    pub terminals: Vec<(Terminal, TerminalShows)>,
    /// What the hub found in the update feed, as a hub installed as a service does. `None` as from one that doesn't
    /// read it.
    pub hub_update: Option<protocol::model::HubUpdate>,
    /// The observations the hub sends at its own level, which it lists with its records, as of the newest revision among
    /// them.
    pub observations: Vec<protocol::model::Observation>,
    /// What the feed tells of observations once a client follows it, in order: what changed after the list was read.
    pub feed: Vec<protocol::client::ObservationNotice>,
    records: i64,
}

#[derive(Clone, Copy)]
pub struct PairingWorld {
    pub paired_device: Option<&'static str>,
}

/// The login capabilities of the hub whose version the world stands for.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub enum LoginSupport {
    None,
    Accounts,
    #[default]
    Defaults,
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

    /// The person did something in the chat at `at_ms`, such as making it or sending the prompt that started a turn,
    /// which the hub notes on its workspace's record, unless they've done something there since. A chat made on its
    /// own, for a test of what's in it, has no workspace here to note it on.
    fn worked_in(&mut self, chat: &SessionId, at_ms: i64) {
        let workspace = &self.sessions.iter().find(|s| &s.id == chat).unwrap().workspace_id;
        let Some(workspace) = self.workspaces.iter_mut().find(|w| &w.id == workspace) else { return };
        workspace.user_active_at_ms = workspace.user_active_at_ms.max(Some(at_ms));
    }

    /// A chat holding these events. Its title is the start of the first prompt, and what's running in the background
    /// is what the newest event to say so says, as the hub makes them.
    pub fn chat(&mut self, workspace: &WorkspaceId, status: SessionStatus, events: Vec<EventBody>) -> SessionId {
        let id = SessionId::from(format!("chat-{}", self.sessions.len() + 1));
        let created_at_ms = self.next_time();
        let title = events.iter().find_map(|body| {
            let EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } = body else { return None };
            Title::from_first_line(text)
        });
        let log = log_of(&id, created_at_ms, events);
        self.subagents.insert(id.clone(), Subagent::recorded_from(&log));
        let last_seq = log.last().map_or(Seq::default(), |event| event.seq);
        let background_tasks =
            log.iter().rev().find_map(|event| event.body.sets_background()).unwrap_or_default().to_vec();
        // With the settings the hub gives a chat that names none.
        self.sessions.push(Session {
            workspace_id: workspace.clone(),
            status,
            last_seq,
            created_at_ms,
            updated_at_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(last_seq.get()).unwrap(),
            title,
            background_tasks,
            ..protocol::fixtures::session(id.as_str())
        });
        self.logs.insert(id.clone(), log);
        self.worked_in(&id, created_at_ms);
        id
    }

    /// Has the agent been working on the chat's turn for this long when the picture is taken.
    pub fn working_for(&mut self, id: &SessionId, so_far_ms: i64) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert_eq!(session.status, SessionStatus::Running, "only a running chat has a turn going");
        session.turn_started_at_ms = Some(NOW_MS - so_far_ms);
        self.worked_in(id, NOW_MS - so_far_ms);
    }

    /// Has the turn that ends a chat's log taken this long by the hub's clock, as the hub says on its end and on the
    /// chat's record.
    pub fn last_turn_took(&mut self, id: &SessionId, took_ms: i64) {
        let ended = self.logs.get_mut(id).and_then(|log| log.last_mut()).unwrap();
        let EventBody::TurnEnded { started_at_ms, .. } = &mut ended.body else { panic!("the log ends mid-turn") };
        *started_at_ms = Some(ended.ts_ms - took_ms);
        let started = *started_at_ms;
        self.sessions.iter_mut().find(|s| &s.id == id).unwrap().turn_started_at_ms = started;
        self.worked_in(id, started.unwrap());
    }

    /// Has the turn that ends the last chat's log run from 11:57 PM to 12:04 AM the night before the picture, by the
    /// hub's clock, as the hub says on its end and on the chat's record.
    #[must_use]
    pub fn last_turn_past_midnight(mut self) -> World {
        let midnight_ms = NOW_MS - 15 * 60 * MINUTE_MS;
        let started_ms = midnight_ms - 3 * MINUTE_MS;
        let session = self.sessions.last_mut().unwrap();
        session.turn_started_at_ms = Some(started_ms);
        let ended = self.logs.get_mut(&session.id).and_then(|log| log.last_mut()).unwrap();
        let EventBody::TurnEnded { started_at_ms, .. } = &mut ended.body else { panic!("the log ends mid-turn") };
        *started_at_ms = Some(started_ms);
        ended.ts_ms = midnight_ms + 4 * MINUTE_MS;
        let chat = session.id.clone();
        self.worked_in(&chat, started_ms);
        self
    }

    /// Has the turn a chat's log ends with end this many minutes before [`NOW_MS`], and three seconds after its agent's
    /// newest request was sent to Opus 5.5 with a 1h cache, as the chat's record says, with its context holding 313.5k
    /// tokens unless it says otherwise. The turn takes as long as it did. It's the chat's newest reply, which has been
    /// read.
    pub fn cached(&mut self, id: &SessionId, minutes_before_now: i64) {
        let used_at_ms = NOW_MS - minutes_before_now * MINUTE_MS;
        let ended = self.logs.get_mut(id).and_then(|log| log.last_mut()).unwrap();
        let EventBody::TurnEnded { started_at_ms, .. } = &mut ended.body else { panic!("the log ends mid-turn") };
        let moved_ms = used_at_ms + 3_000 - ended.ts_ms;
        *started_at_ms = started_at_ms.map(|at| at + moved_ms);
        ended.ts_ms += moved_ms;
        let reply = ended.seq;
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        session.turn_started_at_ms = session.turn_started_at_ms.map(|at| at + moved_ms);
        (session.reply_seq, session.read_seq) = (reply, reply);
        let context = session.context.get_or_insert(ContextUsage {
            used_tokens: 313_500,
            max_tokens: 500_000,
            model_tokens: None,
            max_set_by: None,
            parts: Vec::new(),
        });
        let (model, tokens) = (Some(ModelName::named("claude-opus-5-5")), context.used_tokens);
        session.prompt_cache = Some(PromptCache { used_at_ms, ttl: CacheTtl::OneHour, model, tokens });
    }

    /// Has everything in a chat before its event `seq` happen `idle_ms` earlier, so it had been idle that long when
    /// that one came.
    pub fn idle_before(&mut self, id: &SessionId, seq: Seq, idle_ms: i64) {
        for event in self.logs.get_mut(id).unwrap().iter_mut().filter(|event| event.seq < seq) {
            event.ts_ms -= idle_ms;
            if let EventBody::TurnEnded { started_at_ms, .. } = &mut event.body {
                *started_at_ms = started_at_ms.map(|at| at - idle_ms);
            }
        }
        self.sessions.iter_mut().find(|s| &s.id == id).unwrap().created_at_ms -= idle_ms;
    }

    /// Makes the end of a chat's log its newest reply, which has been read.
    pub fn read_reply(&mut self, id: &SessionId) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert!(session.last_seq > Seq::default(), "a chat with nothing in it has no reply");
        (session.reply_seq, session.read_seq) = (session.last_seq, session.last_seq);
    }

    /// Makes the end of a chat's log a reply nobody has read.
    pub fn unread(&mut self, id: &SessionId) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert!(session.last_seq > Seq::default(), "a chat with nothing in it has no reply");
        (session.reply_seq, session.read_seq) = (session.last_seq, Seq::default());
    }

    /// Closes a chat, this many minutes before [`NOW_MS`].
    pub fn close(&mut self, id: &SessionId, minutes_before_now: i64) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        session.closed_at_ms = Some(NOW_MS - minutes_before_now * MINUTE_MS);
    }

    pub fn session(&self, id: &SessionId) -> Result<&Session, ApiError> {
        self.sessions.iter().find(|s| &s.id == id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such chat"))
    }

    /// How each of the replies read out loud in the chat `id` ended.
    pub fn replies_heard_of(&self, id: &SessionId) -> Result<Vec<ReplyHeard>, ApiError> {
        self.session(id)?;
        Ok(self.replies_heard.get(id).cloned().unwrap_or_default())
    }

    pub fn events_from(&self, id: &SessionId, from_seq: Seq) -> impl Iterator<Item = &SessionEvent> {
        self.logs.get(id).into_iter().flatten().filter(move |event| event.seq >= from_seq)
    }

    /// The agent's own events, or one subagent's, from `from`.
    pub fn placed_from<'a>(
        &'a self,
        id: &SessionId,
        subagent: Option<&'a SubagentId>,
        from: Place,
    ) -> impl Iterator<Item = &'a SessionEvent> {
        let theirs = move |event: &&SessionEvent| event.subagent.as_ref() == subagent && event.place >= from;
        self.logs.get(id).into_iter().flatten().filter(theirs)
    }

    pub fn subagents_of(&self, id: &SessionId) -> Vec<Subagent> {
        self.subagents.get(id).cloned().unwrap_or_default()
    }

    /// The subagent `named` names in the chat `id`, as [`Subagent::named`] finds it, or else `named` as it is, which
    /// reads no events, as a hub takes it.
    pub(super) fn subagent_named(&self, id: &SessionId, named: Option<&SubagentId>) -> Option<SubagentId> {
        let named = named?;
        let found = self.subagents.get(id).and_then(|subagents| Subagent::named(subagents, named));
        Some(found.map_or_else(|| named.clone(), Subagent::id))
    }

    fn next_time(&mut self) -> i64 {
        self.records += 1;
        START_MS + RECORD_GAP_MS * self.records
    }
}

/// A chat's events, with `seq` from 1 and no gaps, and each its place among the events from the same agent or subagent.
fn log_of(id: &SessionId, created_at_ms: i64, events: Vec<EventBody>) -> Vec<SessionEvent> {
    let mut places: HashMap<Option<SubagentId>, u64> = HashMap::new();
    events
        .into_iter()
        .zip((1..).map(Seq::new))
        .map(|(body, seq)| {
            let subagent = body.subagent().cloned();
            let place = places.entry(subagent.clone()).or_default();
            *place += 1;
            SessionEvent {
                session_id: id.clone(),
                ts_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(seq.get()).unwrap(),
                subagent,
                place: Place::new(*place),
                ..protocol::fixtures::event(seq, body)
            }
        })
        .collect()
}
