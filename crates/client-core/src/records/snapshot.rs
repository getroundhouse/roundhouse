//! What a view reads of one hub's records: a copy as of one moment, which doesn't change, and is shared, so holding one
//! costs a pointer. Each kind of record is listed in the order views show it, and found by its ID, and a view keeps the
//! snapshot it last looked at, to tell what's changed since, as [`Snapshot::changes_since`] says.
//!
//! It's made the first time it's asked for after a change, so changes that come between two looks are made into one,
//! and made again only for the side that changed: the records, or the subagents of the sessions views show.

use std::collections::HashMap;
use std::sync::Arc;

use protocol::model::{
    Account, AccountId, DiffComment, HubSettings, NewWorkspaceDraft, Record, Repo, RepoId, Session, SessionId,
    Subagent, ToolUseId, Workspace, WorkspaceId,
};

use super::Taken;
use crate::Agents;

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    records: Arc<Listed>,
    subagents: Arc<SubagentsListed>,
    /// Not a record, but read beside them, as [`super::HubRecords`] says.
    agents: Arc<Agents>,
}

/// Every record but subagents, as views read them.
#[derive(Debug, Default)]
pub(super) struct Listed {
    /// By name.
    repos: Vec<Repo>,
    repo_at: HashMap<RepoId, usize>,
    /// Oldest first, archived ones included.
    workspaces: Vec<Workspace>,
    workspace_at: HashMap<WorkspaceId, (usize, Taken)>,
    /// Each workspace's, oldest first. A workspace with none has no entry.
    sessions: HashMap<WorkspaceId, Group<Session>>,
    session_at: HashMap<SessionId, (WorkspaceId, usize)>,
    /// The same.
    comments: HashMap<WorkspaceId, Group<DiffComment>>,
    accounts: HashMap<AccountId, Account>,
    hub_settings: Option<HubSettings>,
    new_workspace_draft: Option<NewWorkspaceDraft>,
}

/// The subagents of the sessions views asked for, as views read them.
#[derive(Debug, Default)]
pub(super) struct SubagentsListed {
    /// Each session's, in the order they started. Each is shared with what client-core holds, since one can hold a
    /// call's whole input, such as all of a file it wrote, and with what each frame draws.
    sessions: HashMap<SessionId, Group<Arc<Subagent>>>,
    /// Where each is in its session's.
    at: HashMap<SessionId, HashMap<ToolUseId, usize>>,
}

/// Some records that go together, such as a workspace's sessions, and which copies of them these are.
#[derive(Debug)]
struct Group<T> {
    items: Vec<T>,
    copies: Copies,
}

/// Which copies a group holds, which tells two of its groups apart without comparing their records. Each copy taken in
/// is newer than every one before it, so if two snapshots' groups hold as many records, the newest of them the same,
/// every other one in the later group was already held when the earlier was made, and so is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Copies {
    count: usize,
    newest: Taken,
}

impl<T> Group<T> {
    fn of(mut kept: Vec<(T, Taken)>, order: impl Fn(&T, &T) -> std::cmp::Ordering) -> Self {
        kept.sort_by(|(a, _), (b, _)| order(a, b));
        let newest = kept.iter().map(|(_, taken)| *taken).max().unwrap_or_default();
        let copies = Copies { count: kept.len(), newest };
        Group { items: kept.into_iter().map(|(item, _)| item).collect(), copies }
    }
}

impl Listed {
    /// From every record held, with when each copy was taken in.
    pub(super) fn of<'a>(held: impl Iterator<Item = (&'a Record, Taken)>) -> Self {
        let mut listed = Listed::default();
        let (mut workspaces, mut sessions, mut comments) = (Vec::new(), HashMap::new(), HashMap::new());
        for (record, taken) in held {
            match record {
                Record::Repo(repo) => listed.repos.push(repo.clone()),
                Record::Workspace(workspace) => workspaces.push((workspace.clone(), taken)),
                Record::Session(session) => {
                    let its: &mut Vec<_> = sessions.entry(session.workspace_id.clone()).or_default();
                    its.push((session.clone(), taken));
                }
                Record::Comment(comment) => {
                    let its: &mut Vec<_> = comments.entry(comment.workspace_id.clone()).or_default();
                    its.push((comment.clone(), taken));
                }
                Record::HubSettings(settings) => listed.hub_settings = Some(settings.clone()),
                Record::Account(account) => {
                    listed.accounts.insert(account.id.clone(), account.clone());
                }
                Record::NewWorkspaceDraft(draft) => listed.new_workspace_draft = Some(draft.clone()),
                // A subagent is held apart, as `HubRecords::apply_record` says, and a terminal is held, but no view
                // shows terminals yet.
                Record::Subagent(_) | Record::Terminal(_) => {}
            }
        }
        listed.repos.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        listed.repo_at = listed.repos.iter().enumerate().map(|(at, repo)| (repo.id.clone(), at)).collect();
        workspaces.sort_by(|(a, _), (b, _)| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id)));
        listed.workspace_at = workspaces
            .iter()
            .enumerate()
            .map(|(at, (workspace, taken))| (workspace.id.clone(), (at, *taken)))
            .collect();
        listed.workspaces = workspaces.into_iter().map(|(workspace, _)| workspace).collect();
        let oldest_first = |a: &Session, b: &Session| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id));
        listed.sessions = sessions.into_iter().map(|(id, its)| (id, Group::of(its, oldest_first))).collect();
        for (workspace, its) in &listed.sessions {
            let at = its.items.iter().enumerate().map(|(at, session)| (session.id.clone(), (workspace.clone(), at)));
            listed.session_at.extend(at);
        }
        let oldest_first = |a: &DiffComment, b: &DiffComment| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id));
        listed.comments = comments.into_iter().map(|(id, its)| (id, Group::of(its, oldest_first))).collect();
        listed
    }
}

impl SubagentsListed {
    /// From each session's subagents held, with when each copy was taken in.
    pub(super) fn of<'a>(held: impl Iterator<Item = (&'a SessionId, Vec<(Arc<Subagent>, Taken)>)>) -> Self {
        let started =
            |a: &Arc<Subagent>, b: &Arc<Subagent>| (a.started_at_ms, &a.call).cmp(&(b.started_at_ms, &b.call));
        let sessions: HashMap<SessionId, Group<Arc<Subagent>>> =
            held.map(|(session, its)| (session.clone(), Group::of(its, started))).collect();
        let at = sessions.iter().map(|(session, its)| {
            let at = its.items.iter().enumerate().map(|(at, subagent)| (subagent.call.clone(), at));
            (session.clone(), at.collect())
        });
        SubagentsListed { at: at.collect(), sessions }
    }
}

impl Snapshot {
    pub(super) fn new(records: Arc<Listed>, subagents: Arc<SubagentsListed>, agents: Arc<Agents>) -> Self {
        Snapshot { records, subagents, agents }
    }

    /// What each kind of agent the hub runs is called and offers. None until the hub has said, and for good from a hub
    /// too old to say.
    pub fn agents(&self) -> &Agents {
        &self.agents
    }

    /// By name.
    pub fn repos(&self) -> &[Repo] {
        &self.records.repos
    }

    pub fn repo(&self, id: &RepoId) -> Option<&Repo> {
        self.records.repo_at.get(id).map(|&at| &self.records.repos[at])
    }

    /// Oldest first, archived ones included.
    pub fn workspaces(&self) -> &[Workspace] {
        &self.records.workspaces
    }

    pub fn workspace(&self, id: &WorkspaceId) -> Option<&Workspace> {
        self.records.workspace_at.get(id).map(|&(at, _)| &self.records.workspaces[at])
    }

    /// The workspace's sessions, oldest first, closed ones included.
    pub fn sessions(&self, workspace: &WorkspaceId) -> &[Session] {
        self.records.sessions.get(workspace).map_or(&[], |its| &its.items)
    }

    pub fn session(&self, id: &SessionId) -> Option<&Session> {
        let (workspace, at) = self.records.session_at.get(id)?;
        self.records.sessions.get(workspace).map(|its| &its.items[*at])
    }

    /// Every session, whatever its workspace, in no order.
    pub fn every_session(&self) -> impl Iterator<Item = &Session> {
        self.records.sessions.values().flat_map(|its| &its.items)
    }

    /// The comments on the workspace's diff, oldest first.
    pub fn comments(&self, workspace: &WorkspaceId) -> &[DiffComment] {
        self.records.comments.get(workspace).map_or(&[], |its| &its.items)
    }

    /// The record of the account `id`. `None` until it's arrived, and from a hub from before it kept them.
    pub fn account(&self, id: &AccountId) -> Option<&Account> {
        self.records.accounts.get(id)
    }

    /// The hub's own settings. `None` until they've arrived, and for good from a hub from before it kept any.
    pub fn hub_settings(&self) -> Option<&HubSettings> {
        self.records.hub_settings.as_ref()
    }

    /// What's in the new-workspace dialog and not made yet, as the hub keeps it. `None` until it's arrived, and for good
    /// from a hub from before it kept one.
    pub fn new_workspace_draft(&self) -> Option<&NewWorkspaceDraft> {
        self.records.new_workspace_draft.as_ref()
    }

    /// The session's subagents, in the order they started, once a view has asked for them with
    /// [`crate::RecordsFeed::follow_subagents`] and they've been listed.
    pub fn subagents(&self, session: &SessionId) -> &[Arc<Subagent>] {
        self.subagents.sessions.get(session).map_or(&[], |its| &its.items)
    }

    /// The subagent `call` started, in `session`.
    pub fn subagent(&self, session: &SessionId, call: &ToolUseId) -> Option<&Arc<Subagent>> {
        let at = self.subagents.at.get(session)?.get(call)?;
        self.subagents.sessions.get(session).map(|its| &its.items[*at])
    }

    /// What changed between `earlier`, such as the snapshot a view last looked at, and this one.
    pub fn changes_since<'a>(&'a self, earlier: &'a Snapshot) -> Changes<'a> {
        Changes { now: self, then: earlier }
    }
}

/// What changed between two snapshots, by which records: each part a view asks about is a copy of its own, or came or
/// went, in one and not the other. Asking costs the same however many records there are.
pub struct Changes<'a> {
    now: &'a Snapshot,
    then: &'a Snapshot,
}

impl Changes<'_> {
    /// Whether anything changed at all.
    pub fn any(&self) -> bool {
        !Arc::ptr_eq(&self.now.records, &self.then.records)
            || !Arc::ptr_eq(&self.now.subagents, &self.then.subagents)
            || self.agents()
    }

    /// Whether what the hub says of its agents changed, which only a new connection does.
    pub fn agents(&self) -> bool {
        !Arc::ptr_eq(&self.now.agents, &self.then.agents)
    }

    /// Whether the workspace's own record changed.
    pub fn workspace(&self, id: &WorkspaceId) -> bool {
        let taken = |snapshot: &Snapshot| snapshot.records.workspace_at.get(id).map(|&(_, taken)| taken);
        taken(self.now) != taken(self.then)
    }

    /// Whether any of the workspace's sessions changed, came or went.
    pub fn sessions(&self, workspace: &WorkspaceId) -> bool {
        let copies = |snapshot: &Snapshot| snapshot.records.sessions.get(workspace).map(|its| its.copies);
        copies(self.now) != copies(self.then)
    }

    /// Whether any of the comments on the workspace's diff changed, came or went.
    pub fn comments(&self, workspace: &WorkspaceId) -> bool {
        let copies = |snapshot: &Snapshot| snapshot.records.comments.get(workspace).map(|its| its.copies);
        copies(self.now) != copies(self.then)
    }

    /// Whether any of the session's subagents changed, came or went.
    pub fn subagents(&self, session: &SessionId) -> bool {
        let copies = |snapshot: &Snapshot| snapshot.subagents.sessions.get(session).map(|its| its.copies);
        copies(self.now) != copies(self.then)
    }
}

/// One workspace's part of a [`Snapshot`], for a view of that workspace: its record, its sessions and the comments on
/// its diff, with the rest of the snapshot behind them. It's only made for a workspace the snapshot has, so it always
/// has its record.
#[derive(Clone, Debug)]
pub struct InWorkspace {
    all: Snapshot,
    at: usize,
}

impl Snapshot {
    /// The workspace `id`'s part, if this has the workspace.
    pub fn in_workspace(&self, id: &WorkspaceId) -> Option<InWorkspace> {
        let &(at, _) = self.records.workspace_at.get(id)?;
        Some(InWorkspace { all: self.clone(), at })
    }
}

impl InWorkspace {
    pub fn workspace(&self) -> &Workspace {
        &self.all.records.workspaces[self.at]
    }

    /// Its sessions, oldest first, closed ones included.
    pub fn sessions(&self) -> &[Session] {
        self.all.sessions(&self.workspace().id)
    }

    /// One of its sessions.
    pub fn session(&self, id: &SessionId) -> Option<&Session> {
        self.all.session(id).filter(|session| session.workspace_id == self.workspace().id)
    }

    /// The comments on its diff, oldest first.
    pub fn comments(&self) -> &[DiffComment] {
        self.all.comments(&self.workspace().id)
    }

    /// The whole snapshot it's part of, such as for the accounts its sessions name and their subagents.
    pub fn all(&self) -> &Snapshot {
        &self.all
    }
}

#[cfg(test)]
mod tests;
