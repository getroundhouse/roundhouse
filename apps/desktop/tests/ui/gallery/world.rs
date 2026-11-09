//! Everything the fake hub serves, and the builders that make it. Records get IDs and times in the order they're
//! made, so a screen gets the same data on every run.

#![cfg(test)]

use std::collections::HashMap;

use protocol::client::ApiError;
use protocol::client::ErrorCode;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{
    AgentKind, DiffComment, GitDiff, InstanceId, PullRequest, Records, Repo, RepoId, Rev, Seq, Session, SessionId,
    SessionStatus, SetupStatus, Workspace, WorkspaceId, WorkspaceStatus,
};
use protocol::tail::Tail;

/// When the first record was made: 1 November 2026. Each one after it is a minute later, and each event a second.
const START_MS: i64 = 1_793_491_200_000;
const RECORD_GAP_MS: i64 = 60_000;
const EVENT_GAP_MS: i64 = 1_000;

#[derive(Default)]
pub(crate) struct World {
    pub(crate) repos: Vec<Repo>,
    pub(crate) workspaces: Vec<Workspace>,
    pub(crate) sessions: Vec<Session>,
    /// Each session's events, with `seq` from 1 and no gaps, as the protocol promises.
    logs: HashMap<SessionId, Vec<SessionEvent>>,
    /// What's still streaming, for a chat caught mid-reply.
    pub(crate) tails: HashMap<SessionId, Tail>,
    pub(crate) diffs: HashMap<WorkspaceId, GitDiff>,
    pub(crate) comments: HashMap<WorkspaceId, Vec<DiffComment>>,
    pub(crate) setup_logs: HashMap<WorkspaceId, String>,
    pub(crate) pull_requests: HashMap<WorkspaceId, PullRequest>,
    /// The hub's build, for a screen about a hub that isn't the app's build.
    pub(crate) hub: Option<HubBuild>,
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
        let port_base = 4000 + 10 * u16::try_from(self.workspaces.len()).unwrap();
        self.workspaces.push(Workspace {
            id: id.clone(),
            repo_id: repo.clone(),
            name: name.to_string(),
            branch: name.to_string(),
            base_ref: Some("origin/main".to_string()),
            path: Some(format!("/workspaces/{name}")),
            status: WorkspaceStatus::Ready,
            provider: "local".to_string(),
            instance_id: InstanceId::from("local"),
            created_at_ms,
            archived_at_ms: None,
            setup: SetupStatus::Succeeded,
            port_base,
            rev: Rev::default(),
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
            rev: Rev::default(),
        }
    }

    pub(crate) fn workspace_mut(&mut self, id: &WorkspaceId) -> &mut Workspace {
        self.workspaces.iter_mut().find(|w| &w.id == id).unwrap()
    }

    /// A chat holding these events. Its title is the start of the first prompt, as the hub makes it.
    pub(crate) fn chat(&mut self, workspace: &WorkspaceId, status: SessionStatus, events: Vec<EventBody>) -> SessionId {
        let id = SessionId::from(format!("chat-{}", self.sessions.len() + 1));
        let created_at_ms = self.next_time();
        let title = events.iter().find_map(|body| match body {
            EventBody::UserPrompt { text, .. } => Some(text.chars().take(60).collect()),
            _ => None,
        });
        let log: Vec<SessionEvent> = events
            .into_iter()
            .zip(1..)
            .map(|(body, seq): (EventBody, Seq)| SessionEvent {
                session_id: id.clone(),
                seq,
                ts_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(seq).unwrap(),
                run: None,
                body,
                raw: None,
            })
            .collect();
        let last_seq = log.last().map_or(0, |event| event.seq);
        self.sessions.push(Session {
            id: id.clone(),
            workspace_id: workspace.clone(),
            agent: AgentKind::Claude,
            model: Some("opus".to_string()),
            permission_mode: Some("acceptEdits".to_string()),
            status,
            last_seq,
            created_at_ms,
            updated_at_ms: created_at_ms + EVENT_GAP_MS * i64::try_from(last_seq).unwrap(),
            title,
            closed_at_ms: None,
            rev: Rev::default(),
        });
        self.logs.insert(id.clone(), log);
        id
    }

    pub(crate) fn session(&self, id: &SessionId) -> Result<&Session, ApiError> {
        self.sessions.iter().find(|s| &s.id == id).ok_or_else(|| ApiError::new(ErrorCode::NotFound, "no such chat"))
    }

    pub(crate) fn events_from(&self, id: &SessionId, from_seq: Seq) -> impl Iterator<Item = &SessionEvent> {
        self.logs.get(id).into_iter().flatten().filter(move |event| event.seq >= from_seq)
    }

    fn next_time(&mut self) -> i64 {
        self.records += 1;
        START_MS + RECORD_GAP_MS * self.records
    }
}
