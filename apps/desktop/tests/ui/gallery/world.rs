//! Everything the fake hub serves, and the builders that make it. Records get IDs and times in the order they're
//! made, so a screen gets the same data on every run.

#![cfg(test)]

use std::collections::HashMap;

use protocol::client::ApiError;
use protocol::client::ErrorCode;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{
    Account, AttachmentId, CacheTtl, ContextUsage, DiffComment, FileContents, GitDiff, InstanceId, ModelName,
    PORTS_PER_WORKSPACE, Place, PromptCache, PullRequest, Records, Repo, RepoId, Rev, Seq, Session, SessionId,
    SessionStatus, Subagent, Title, ToolUseId, Workspace, WorkspaceId, WorktreeFiles,
};
use protocol::prompt::Prompt;
use protocol::tail::Tail;

use super::subagent_records::records_of;

/// When the first record was made: 1 November 2026, at 2 PM in UTC, which pictures show times in. Each one after it is
/// a minute later, and each event a second.
const START_MS: i64 = 1_793_541_600_000;
const RECORD_GAP_MS: i64 = 60_000;
const EVENT_GAP_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60_000;
/// What the app takes as the time now, in every picture: an hour after the first record, so later than any record a
/// world makes, and on the same day, so a turn that ended says only its time.
pub(crate) const NOW_MS: i64 = START_MS + 60 * MINUTE_MS;

#[derive(Default)]
pub(crate) struct World {
    pub(crate) repos: Vec<Repo>,
    pub(crate) workspaces: Vec<Workspace>,
    pub(crate) sessions: Vec<Session>,
    /// Each session's events, with `seq` from 1 and no gaps, as the protocol promises, and each its place among the
    /// events from the same agent or subagent.
    logs: HashMap<SessionId, Vec<SessionEvent>>,
    /// Each session's subagents, made from its events.
    subagents: HashMap<SessionId, Vec<Subagent>>,
    /// What's still streaming, for a chat caught mid-reply.
    pub(crate) tails: HashMap<SessionId, Tail>,
    pub(crate) diffs: HashMap<WorkspaceId, GitDiff>,
    pub(crate) comments: HashMap<WorkspaceId, Vec<DiffComment>>,
    pub(crate) setup_logs: HashMap<WorkspaceId, String>,
    pub(crate) pull_requests: HashMap<WorkspaceId, PullRequest>,
    /// The bytes of the images the hub keeps, by attachment: the files prompts carry, and the images tools send back
    /// and the agent's text points to.
    pub(crate) attachments: HashMap<AttachmentId, Vec<u8>>,
    /// The files and folders in the screen's worktree, as All files lists them.
    pub(crate) worktree: WorktreeFiles,
    /// What's in each folder of the screen's worktree that's read when it's opened, by its path.
    pub(crate) worktree_folders: HashMap<String, WorktreeFiles>,
    /// What the files in the screen's worktree that can be opened hold, by path.
    pub(crate) worktree_files: HashMap<String, FileContents>,
    /// What the files the screen's diff changes held in the commit it was taken from, by path.
    pub(crate) base_files: HashMap<String, FileContents>,
    /// The hub's build, for a screen about a hub that isn't the app's build.
    pub(crate) hub: Option<HubBuild>,
    /// The records of the accounts the screen's chats run under.
    pub(crate) accounts: Vec<Account>,
    /// The hub is from before hubs kept settings, so it sends none.
    pub(crate) keeps_no_settings: bool,
    records: i64,
}

/// A hub's build, other than the app's.
#[derive(Clone, Copy)]
pub(crate) struct HubBuild {
    pub(crate) version: &'static str,
    pub(crate) number: u64,
    /// A client started it, rather than someone running it themselves.
    pub(crate) on_demand: bool,
}

impl World {
    pub(crate) fn repo(&mut self, name: &str) -> RepoId {
        let id = RepoId::from(format!("repo-{}", self.repos.len() + 1));
        let created_at_ms = self.next_time();
        self.repos.push(Repo {
            id: id.clone(),
            name: name.to_string(),
            path: format!("/src/{name}"),
            created_at_ms,
            rev: Rev::default(),
        });
        id
    }

    /// Ready, with setup done. Change what a screen needs through [`World::workspace_mut`].
    pub(crate) fn workspace(&mut self, repo: &RepoId, name: &str) -> WorkspaceId {
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

    /// Everything, as one list.
    pub(crate) fn records(&self) -> Records {
        Records {
            repos: self.repos.clone(),
            workspaces: self.workspaces.clone(),
            sessions: self.sessions.clone(),
            comments: self.comments.values().flatten().cloned().collect(),
            // As a hub starts out.
            hub_settings: (!self.keeps_no_settings)
                .then(|| protocol::model::HubSettings { keep_awake: true, rev: Rev::default() }),
            accounts: self.accounts.clone(),
            // As from a hub that doesn't keep one, since its hub refuses to.
            new_workspace_draft: None,
            rev: Rev::default(),
        }
    }

    pub(crate) fn workspace_mut(&mut self, id: &WorkspaceId) -> &mut Workspace {
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
    /// is the newest list, as the hub makes them.
    pub(crate) fn chat(&mut self, workspace: &WorkspaceId, status: SessionStatus, events: Vec<EventBody>) -> SessionId {
        let id = SessionId::from(format!("chat-{}", self.sessions.len() + 1));
        let created_at_ms = self.next_time();
        let title = events.iter().find_map(|body| match body {
            EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } => Title::from_first_line(text),
            _ => None,
        });
        let log = log_of(&id, created_at_ms, events);
        self.subagents.insert(id.clone(), records_of(&log));
        let last_seq = log.last().map_or(0, |event| event.seq);
        let background_tasks = log
            .iter()
            .rev()
            .find_map(|event| match &event.body {
                EventBody::AgentTasks { tasks, .. } => Some(tasks.clone()),
                _ => None,
            })
            .unwrap_or_default();
        // With the settings the hub gives a chat that names none.
        self.sessions.push(Session {
            workspace_id: workspace.clone(),
            status,
            last_seq,
            created_at_ms,
            updated_at_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(last_seq).unwrap(),
            title,
            background_tasks,
            ..protocol::fixtures::session(id.as_str())
        });
        self.logs.insert(id.clone(), log);
        self.worked_in(&id, created_at_ms);
        id
    }

    /// Has the agent been working on the chat's turn for this long when the picture is taken.
    pub(crate) fn working_for(&mut self, id: &SessionId, so_far_ms: i64) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert_eq!(session.status, SessionStatus::Running, "only a running chat has a turn going");
        session.turn_started_at_ms = Some(NOW_MS - so_far_ms);
        self.worked_in(id, NOW_MS - so_far_ms);
    }

    /// Has the turn that ends a chat's log taken this long by the hub's clock, as the hub says on its end and on the
    /// chat's record.
    pub(crate) fn last_turn_took(&mut self, id: &SessionId, took_ms: i64) {
        let ended = self.logs.get_mut(id).and_then(|log| log.last_mut()).unwrap();
        let EventBody::TurnEnded { started_at_ms, .. } = &mut ended.body else { panic!("the log ends mid-turn") };
        *started_at_ms = Some(ended.ts_ms - took_ms);
        let started = *started_at_ms;
        self.sessions.iter_mut().find(|s| &s.id == id).unwrap().turn_started_at_ms = started;
        self.worked_in(id, started.unwrap());
    }

    /// Has the turn that ends the last chat's log run from 11:57 PM to 12:04 AM the night before the picture, by the
    /// hub's clock, as the hub says on its end and on the chat's record.
    pub(crate) fn last_turn_past_midnight(mut self) -> World {
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
    pub(crate) fn cached(&mut self, id: &SessionId, minutes_before_now: i64) {
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
    pub(crate) fn idle_before(&mut self, id: &SessionId, seq: Seq, idle_ms: i64) {
        for event in self.logs.get_mut(id).unwrap().iter_mut().filter(|event| event.seq < seq) {
            event.ts_ms -= idle_ms;
            if let EventBody::TurnEnded { started_at_ms, .. } = &mut event.body {
                *started_at_ms = started_at_ms.map(|at| at - idle_ms);
            }
        }
        self.sessions.iter_mut().find(|s| &s.id == id).unwrap().created_at_ms -= idle_ms;
    }

    /// Makes the end of a chat's log a reply nobody has read.
    pub(crate) fn unread(&mut self, id: &SessionId) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        assert!(session.last_seq > 0, "a chat with nothing in it has no reply");
        (session.reply_seq, session.read_seq) = (session.last_seq, 0);
    }

    /// Closes a chat, this many minutes before [`NOW_MS`].
    pub(crate) fn close(&mut self, id: &SessionId, minutes_before_now: i64) {
        let session = self.sessions.iter_mut().find(|s| &s.id == id).unwrap();
        session.closed_at_ms = Some(NOW_MS - minutes_before_now * MINUTE_MS);
    }

    pub(crate) fn session(&self, id: &SessionId) -> Result<&Session, ApiError> {
        self.sessions.iter().find(|s| &s.id == id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such chat"))
    }

    pub(crate) fn events_from(&self, id: &SessionId, from_seq: Seq) -> impl Iterator<Item = &SessionEvent> {
        self.logs.get(id).into_iter().flatten().filter(move |event| event.seq >= from_seq)
    }

    /// The agent's own events, or one subagent's, from `from`.
    pub(crate) fn placed_from<'a>(
        &'a self,
        id: &SessionId,
        subagent: Option<&'a ToolUseId>,
        from: Place,
    ) -> impl Iterator<Item = &'a SessionEvent> {
        let theirs = move |event: &&SessionEvent| event.subagent.as_ref() == subagent && event.place >= from;
        self.logs.get(id).into_iter().flatten().filter(theirs)
    }

    pub(crate) fn subagents_of(&self, id: &SessionId) -> Vec<Subagent> {
        self.subagents.get(id).cloned().unwrap_or_default()
    }

    fn next_time(&mut self) -> i64 {
        self.records += 1;
        START_MS + RECORD_GAP_MS * self.records
    }
}

/// A chat's events, with `seq` from 1 and no gaps, and each its place among the events from the same agent or subagent.
fn log_of(id: &SessionId, created_at_ms: i64, events: Vec<EventBody>) -> Vec<SessionEvent> {
    let mut places: HashMap<Option<ToolUseId>, u64> = HashMap::new();
    events
        .into_iter()
        .zip(1..)
        .map(|(body, seq): (EventBody, Seq)| {
            let subagent = body.subagent().cloned();
            let place = places.entry(subagent.clone()).or_default();
            *place += 1;
            SessionEvent {
                session_id: id.clone(),
                ts_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(seq).unwrap(),
                subagent,
                place: Place::new(*place),
                ..protocol::fixtures::event(seq, body)
            }
        })
        .collect()
}
