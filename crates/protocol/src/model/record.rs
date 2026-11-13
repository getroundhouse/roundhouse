//! What a record is, of any kind: which one it is, its revision, and every record a hub holds, read at one moment. A
//! new kind of record is added here, and the compiler lists what else has to name it, so nothing that keeps records
//! has to list their kinds.

use serde::{Deserialize, Serialize};

use super::{
    Account, AccountId, CommentId, DiffComment, HubSettings, NewWorkspaceDraft, Repo, RepoId, Rev, Session, SessionId,
    Subagent, ToolUseId, Workspace, WorkspaceId,
};

/// Which record, of which kind. A kind this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordKey {
    Repo {
        id: RepoId,
    },
    Workspace {
        id: WorkspaceId,
    },
    Session {
        id: SessionId,
    },
    Comment {
        id: CommentId,
    },
    /// A hub has one record of its settings.
    HubSettings,
    /// And one of what's in the new-workspace dialog, as [`NewWorkspaceDraft`] says.
    NewWorkspaceDraft,
    Account {
        id: AccountId,
    },
    /// A subagent has no ID of its own: it's the call that started it, in its session. The hub never removes one, so
    /// this isn't sent, and a peer from before it would read it as `Unknown`.
    Subagent {
        session_id: SessionId,
        call: ToolUseId,
    },
    #[serde(other)]
    Unknown,
}

/// One record of any kind, as a list or the feed gives it.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Repo(Repo),
    Workspace(Workspace),
    Session(Session),
    Comment(DiffComment),
    HubSettings(HubSettings),
    Account(Account),
    NewWorkspaceDraft(NewWorkspaceDraft),
    /// Listed by session, apart from [`Records`], since a session can have many.
    Subagent(Subagent),
}

impl Record {
    pub fn key(&self) -> RecordKey {
        match self {
            Record::Repo(repo) => RecordKey::Repo { id: repo.id.clone() },
            Record::Workspace(workspace) => RecordKey::Workspace { id: workspace.id.clone() },
            Record::Session(session) => RecordKey::Session { id: session.id.clone() },
            Record::Comment(comment) => RecordKey::Comment { id: comment.id.clone() },
            Record::HubSettings(_) => RecordKey::HubSettings,
            Record::Account(account) => RecordKey::Account { id: account.id.clone() },
            Record::NewWorkspaceDraft(_) => RecordKey::NewWorkspaceDraft,
            Record::Subagent(subagent) => {
                RecordKey::Subagent { session_id: subagent.session_id.clone(), call: subagent.call.clone() }
            }
        }
    }

    /// The revision of this copy, which says whether it replaces another, as [`Rev::supersedes`] does.
    pub fn rev(&self) -> Rev {
        match self {
            Record::Repo(repo) => repo.rev,
            Record::Workspace(workspace) => workspace.rev,
            Record::Session(session) => session.rev,
            Record::Comment(comment) => comment.rev,
            Record::HubSettings(settings) => settings.rev,
            Record::Account(account) => account.rev,
            Record::NewWorkspaceDraft(draft) => draft.rev,
            Record::Subagent(subagent) => subagent.rev,
        }
    }
}

/// Every record a hub holds, read at one moment, but for subagents, which are listed by session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Records {
    pub repos: Vec<Repo>,
    pub workspaces: Vec<Workspace>,
    pub sessions: Vec<Session>,
    pub comments: Vec<DiffComment>,
    /// `None` from a hub from before it kept any.
    #[serde(default)]
    pub hub_settings: Option<HubSettings>,
    /// Empty from a hub from before it kept them.
    #[serde(default)]
    pub accounts: Vec<Account>,
    /// `None` from a hub from before it kept one.
    #[serde(default)]
    pub new_workspace_draft: Option<NewWorkspaceDraft>,
    /// The hub's newest revision when this was read. A record missing from the list is gone, unless
    /// a client holds a copy newer than this.
    pub rev: Rev,
}

impl Records {
    /// Each record in the list, of every kind. The list's own revision is `rev`, which this leaves out.
    pub fn into_records(self) -> impl Iterator<Item = Record> {
        let Records { repos, workspaces, sessions, comments, hub_settings, accounts, new_workspace_draft, rev: _ } =
            self;
        let records = repos.into_iter().map(Record::Repo);
        let records = records.chain(workspaces.into_iter().map(Record::Workspace));
        let records = records.chain(sessions.into_iter().map(Record::Session));
        let records = records.chain(comments.into_iter().map(Record::Comment));
        let records = records.chain(hub_settings.into_iter().map(Record::HubSettings));
        let records = records.chain(accounts.into_iter().map(Record::Account));
        records.chain(new_workspace_draft.into_iter().map(Record::NewWorkspaceDraft))
    }
}
