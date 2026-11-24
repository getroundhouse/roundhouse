use super::super::Taken;
use super::by_session::SessionsListed;
use super::github::GithubListed;
use super::observed::ObservationsListed;
use crate::Agents;
use protocol::model::{
    Account, AccountId, DiffComment, HubSettings, HubUpdate, MessageId, ModelList, ModelListKey, NewWorkspaceDraft,
    Proposal, ProposalId, Reach, Record, ReplyHeard, Repo, RepoId, Session, SessionId, Subagent, SubagentId, Terminal,
    Workspace, WorkspaceId,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub(super) records: Arc<Listed>,
    pub(super) observations: Arc<ObservationsListed>,
    pub(super) subagents: Arc<SessionsListed<Subagent>>,
    pub(super) proposals: Arc<SessionsListed<Proposal>>,
    pub(super) replies_heard: Arc<SessionsListed<ReplyHeard>>,
    /// Not a record, but read beside them, as [`super::super::HubRecords`] says.
    pub(super) agents: Arc<Agents>,
}

/// Every record but those listed by session, as views read them.
#[derive(Debug, Default)]
pub(in super::super) struct Listed {
    /// By name.
    pub(super) repos: Vec<Repo>,
    pub(super) repo_at: HashMap<RepoId, usize>,
    /// Oldest first, archived ones included.
    pub(super) workspaces: Vec<Workspace>,
    pub(super) workspace_at: HashMap<WorkspaceId, (usize, Taken)>,
    /// Each workspace's, oldest first. A workspace with none has no entry.
    pub(super) sessions: HashMap<WorkspaceId, Group<Session>>,
    pub(super) session_at: HashMap<SessionId, (WorkspaceId, usize)>,
    /// The same.
    pub(super) comments: HashMap<WorkspaceId, Group<DiffComment>>,
    /// Each workspace's, in the order they opened.
    pub(super) terminals: HashMap<WorkspaceId, Group<Terminal>>,
    pub(super) accounts: HashMap<AccountId, Account>,
    pub(super) model_lists: HashMap<ModelListKey, ModelList>,
    pub(super) hub_settings: Option<HubSettings>,
    pub(super) hub_update: Option<HubUpdate>,
    pub(super) new_workspace_draft: Option<NewWorkspaceDraft>,
    pub(super) github: GithubListed,
}

/// Some records that go together, such as a workspace's sessions, and which copies of them these are.
#[derive(Debug)]
pub(super) struct Group<T> {
    pub(super) items: Vec<T>,
    pub(super) copies: Copies,
}

/// Which copies a group holds, which tells two of its groups apart without comparing their records. Each copy taken in
/// is newer than every one before it, so if two snapshots' groups hold as many records, the newest of them the same,
/// every other one in the later group was already held when the earlier was made, and so is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Copies {
    pub(super) count: usize,
    pub(super) newest: Taken,
}

impl<T> Group<T> {
    pub(super) fn of(mut kept: Vec<(T, Taken)>, order: impl Fn(&T, &T) -> std::cmp::Ordering) -> Self {
        kept.sort_by(|(a, _), (b, _)| order(a, b));
        let newest = kept.iter().map(|(_, taken)| *taken).max().unwrap_or_default();
        let copies = Copies { count: kept.len(), newest };
        Group { items: kept.into_iter().map(|(item, _)| item).collect(), copies }
    }
}

impl Listed {
    /// From every record held, with when each copy was taken in.
    pub(in super::super) fn of<'a>(held: impl Iterator<Item = (&'a Record, Taken)>) -> Self {
        let mut listed = Listed::default();
        let (mut workspaces, mut sessions, mut comments, mut terminals) =
            (Vec::new(), HashMap::new(), HashMap::new(), HashMap::new());
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
                Record::HubUpdate(update) => listed.hub_update = Some(update.clone()),
                Record::Account(account) => {
                    listed.accounts.insert(account.id.clone(), account.clone());
                }
                Record::NewWorkspaceDraft(draft) => listed.new_workspace_draft = Some(draft.clone()),
                Record::ModelList(list) => {
                    listed.model_lists.insert(list.key.clone(), list.clone());
                }
                Record::GithubAccount(account) => listed.github.account(account),
                Record::GithubSignIn(sign_in) => listed.github.sign_in(sign_in),
                Record::GithubApp(app) => listed.github.app(app),
                Record::GithubReach(reach) => listed.github.reach(reach),
                Record::Terminal(terminal) => {
                    let its: &mut Vec<_> = terminals.entry(terminal.workspace_id.clone()).or_default();
                    its.push((terminal.clone(), taken));
                }
                // Held apart, as `HubRecords::apply_record` says, but for a found login, from a hub from before
                // Settings' Providers, which no view shows.
                Record::Subagent(_) | Record::Proposal(_) | Record::ReplyHeard(_) | Record::FoundGithubLogin(_) => {}
            }
        }
        listed.github.order();
        listed.repos.sort_by(Repo::by_name);
        listed.repo_at = listed.repos.iter().enumerate().map(|(at, repo)| (repo.id.clone(), at)).collect();
        workspaces.sort_by(|(a, _), (b, _)| a.oldest_first(b));
        listed.workspace_at = workspaces
            .iter()
            .enumerate()
            .map(|(at, (workspace, taken))| (workspace.id.clone(), (at, *taken)))
            .collect();
        listed.workspaces = workspaces.into_iter().map(|(workspace, _)| workspace).collect();
        listed.sessions = sessions.into_iter().map(|(id, its)| (id, Group::of(its, Session::oldest_first))).collect();
        for (workspace, its) in &listed.sessions {
            let at = its.items.iter().enumerate().map(|(at, session)| (session.id.clone(), (workspace.clone(), at)));
            listed.session_at.extend(at);
        }
        listed.comments =
            comments.into_iter().map(|(id, its)| (id, Group::of(its, DiffComment::oldest_first))).collect();
        let first_opened_first = |a: &Terminal, b: &Terminal| (a.opened_at_ms, &a.id).cmp(&(b.opened_at_ms, &b.id));
        listed.terminals = terminals.into_iter().map(|(id, its)| (id, Group::of(its, first_opened_first))).collect();
        listed
    }
}

impl Snapshot {
    pub(in super::super) fn new(
        records: Arc<Listed>,
        observations: Arc<ObservationsListed>,
        subagents: Arc<SessionsListed<Subagent>>,
        proposals: Arc<SessionsListed<Proposal>>,
        replies_heard: Arc<SessionsListed<ReplyHeard>>,
        agents: Arc<Agents>,
    ) -> Self {
        Snapshot { records, observations, subagents, proposals, replies_heard, agents }
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

    /// The workspace's open terminals, in the order they opened.
    pub fn terminals(&self, workspace: &WorkspaceId) -> &[Terminal] {
        self.records.terminals.get(workspace).map_or(&[], |its| &its.items)
    }

    /// The record of the account `id`. `None` until it's arrived, and from a hub from before it kept them.
    pub fn account(&self, id: &AccountId) -> Option<&Account> {
        self.records.accounts.get(id)
    }

    /// Every account's record, in no order. Empty until they've arrived, and from a hub from before it kept them.
    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.records.accounts.values()
    }

    /// The hub's own settings. `None` until they've arrived, and for good from a hub from before it kept any.
    pub fn hub_settings(&self) -> Option<&HubSettings> {
        self.records.hub_settings.as_ref()
    }

    /// The release the hub would update to, as it last found it in the update feed. `None` until it's arrived, and for
    /// good from a hub that doesn't read the feed, as [`HubUpdate`] says.
    pub fn hub_update(&self) -> Option<&HubUpdate> {
        self.records.hub_update.as_ref()
    }

    /// What's in the new-workspace dialog and not made yet, as the hub keeps it. `None` until it's arrived, and for good
    /// from a hub from before it kept one.
    pub fn new_workspace_draft(&self) -> Option<&NewWorkspaceDraft> {
        self.records.new_workspace_draft.as_ref()
    }

    /// The session's subagents, in the order they started, or last started again, while the client follows the session,
    /// as an [`crate::OpenSession`] does, once its subscription has listed them.
    pub fn subagents(&self, session: &SessionId) -> &[Arc<Subagent>] {
        self.subagents.of_session(session)
    }

    /// The subagent `subagent`, in `session`.
    pub fn subagent(&self, session: &SessionId, subagent: &SubagentId) -> Option<&Arc<Subagent>> {
        self.subagents.find(session, subagent)
    }

    /// The session's proposals, in the order they were made, while the client follows the session, once its
    /// subscription has listed them.
    pub fn proposals(&self, session: &SessionId) -> &[Arc<Proposal>] {
        self.proposals.of_session(session)
    }

    /// The proposal `proposal`, in `session`.
    pub fn proposal(&self, session: &SessionId, proposal: &ProposalId) -> Option<&Arc<Proposal>> {
        self.proposals.find(session, proposal)
    }

    /// How the reply `message`, in `session`, ended for the person hearing it, while the client follows the session,
    /// once its subscription has listed the session's records. `None` for a reply that wasn't read out, or hasn't ended.
    pub fn reply_heard(&self, session: &SessionId, message: &MessageId) -> Option<&Arc<ReplyHeard>> {
        self.replies_heard.find(session, message)
    }

    /// How each of the session's replies read out loud ended, as [`Snapshot::reply_heard`] says of one, by reply.
    pub fn replies_heard(&self, session: &SessionId) -> &[Arc<ReplyHeard>] {
        self.replies_heard.of_session(session)
    }

    /// What changed between `earlier`, such as the snapshot a view last looked at, and this one.
    pub fn changes_since<'a>(&'a self, earlier: &'a Snapshot) -> Changes<'a> {
        Changes { now: self, then: earlier }
    }
}

/// What changed between two snapshots, by which records: each part a view asks about is a copy of its own, or came or
/// went, in one and not the other. Asking costs the same however many records there are.
pub struct Changes<'a> {
    pub(super) now: &'a Snapshot,
    pub(super) then: &'a Snapshot,
}

impl Changes<'_> {
    /// Whether anything changed at all.
    pub fn any(&self) -> bool {
        !Arc::ptr_eq(&self.now.records, &self.then.records)
            || !Arc::ptr_eq(&self.now.observations, &self.then.observations)
            || !Arc::ptr_eq(&self.now.subagents, &self.then.subagents)
            || !Arc::ptr_eq(&self.now.proposals, &self.then.proposals)
            || !Arc::ptr_eq(&self.now.replies_heard, &self.then.replies_heard)
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

    /// Whether any of the workspace's terminals changed, came or went.
    pub fn terminals(&self, workspace: &WorkspaceId) -> bool {
        let copies = |snapshot: &Snapshot| snapshot.records.terminals.get(workspace).map(|its| its.copies);
        copies(self.now) != copies(self.then)
    }

    /// Whether any of the session's subagents changed, came or went.
    pub fn subagents(&self, session: &SessionId) -> bool {
        self.now.subagents.copies(session) != self.then.subagents.copies(session)
    }

    /// Whether any of the session's proposals changed, came or went.
    pub fn proposals(&self, session: &SessionId) -> bool {
        self.now.proposals.copies(session) != self.then.proposals.copies(session)
    }

    /// Whether how any of the session's replies was heard changed, or was recorded.
    pub fn replies_heard(&self, session: &SessionId) -> bool {
        self.now.replies_heard.copies(session) != self.then.replies_heard.copies(session)
    }
}

/// One workspace's part of a [`Snapshot`], for a view of that workspace: its record, its sessions, the comments on its
/// diff and its terminals, with the rest of the snapshot behind them. It's only made for a workspace the snapshot has,
/// so it always has its record.
#[derive(Clone, Debug)]
pub struct InWorkspace {
    pub(super) all: Snapshot,
    pub(super) at: usize,
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

    /// What a push from it would meet, which is its repository's, as [`Snapshot::github_reach`] says, on a provider
    /// where Roundhouse sets up GitHub. `None` on one whose machines the hub's settings say it leaves GitHub to, as
    /// [`GithubSettings::left_to_machine`](protocol::model::GithubSettings::left_to_machine) says, whose pushes go as
    /// the machine sends them, not as the default.
    pub fn github_reach(&self) -> Option<&Reach> {
        let workspace = self.workspace();
        let github = self.all.hub_settings().and_then(|settings| settings.github.as_ref());
        if github.is_some_and(|github| !github.sets_up(&workspace.provider)) {
            return None;
        }
        self.all.github_reach(&workspace.repo_id)
    }

    /// The comments on its diff, oldest first.
    pub fn comments(&self) -> &[DiffComment] {
        self.all.comments(&self.workspace().id)
    }

    /// Its open terminals, in the order they opened.
    pub fn terminals(&self) -> &[Terminal] {
        self.all.terminals(&self.workspace().id)
    }

    /// The whole snapshot it's part of, such as for the accounts its sessions name, their subagents and their
    /// proposals.
    pub fn all(&self) -> &Snapshot {
        &self.all
    }
}
