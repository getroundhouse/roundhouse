//! What a record is, of any kind: which one it is, its revision, and every record a hub holds, read at one moment. A
//! new kind of record is added here, and the compiler lists what else has to name it, so nothing that keeps records
//! has to list their kinds.

use serde::{Deserialize, Serialize};

use super::{
    Account, AccountId, CommentId, DiffComment, GithubAccount, GithubAccountId, GithubApp, GithubAppId, GithubSignIn,
    GithubSignInId, HubSettings, HubUpdate, MessageId, ModelList, ModelListKey, NewWorkspaceDraft, Observation,
    Proposal, ProposalId, ReplyHeard, Repo, RepoId, Rev, Session, SessionId, Subagent, SubagentId, Terminal,
    TerminalId, ToolServer, ToolServerId, Workspace, WorkspaceId,
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
    /// A subagent, in its session. The hub never removes one, so this isn't sent. Its ID is written `call`, as it was
    /// when a subagent was named by the call that started it.
    Subagent {
        session_id: SessionId,
        #[serde(rename = "call")]
        subagent: SubagentId,
    },
    /// Sent once it has been disconnected, unless it's an app's installation, whose record stays until its app is
    /// removed, as [`GithubAccountDisconnect`](crate::client::github_accounts::GithubAccountDisconnect) says.
    GithubAccount {
        id: GithubAccountId,
    },
    /// Sent once it has gone, as [`GithubSignIn`] says.
    GithubSignIn {
        id: GithubSignInId,
    },
    /// Sent once it has been removed, as [`GithubApp`] says.
    GithubApp {
        id: GithubAppId,
    },
    /// Sent once it has been removed, as [`ToolServerRemove`](crate::client::tool_servers::ToolServerRemove) says.
    ToolServer {
        id: ToolServerId,
    },
    Terminal {
        id: TerminalId,
    },
    /// Sent once the hub has let go of it, as [`ModelList`] says.
    ModelList(ModelListKey),
    /// A hub has at most one, which it never removes once it has it, as [`HubUpdate`] says, so this isn't sent.
    HubUpdate,
    /// A proposal. The hub never removes one while its chat is there, so this isn't sent.
    Proposal {
        id: ProposalId,
    },
    /// How one of a session's replies said out loud ended, by the reply. The hub never removes one while its chat is
    /// there, so this isn't sent.
    ReplyHeard {
        session_id: SessionId,
        message: MessageId,
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
    GithubAccount(GithubAccount),
    GithubSignIn(GithubSignIn),
    GithubApp(GithubApp),
    ToolServer(ToolServer),
    Terminal(Terminal),
    ModelList(ModelList),
    HubUpdate(HubUpdate),
    /// Listed by session, apart from [`Records`], as subagents are, since each holds what its agent wrote in full.
    Proposal(Proposal),
    /// Listed by session, apart from [`Records`], as proposals are, since only the sessions a view shows need them.
    ReplyHeard(ReplyHeard),
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
                RecordKey::Subagent { session_id: subagent.session_id.clone(), subagent: subagent.id.clone() }
            }
            Record::GithubAccount(account) => RecordKey::GithubAccount { id: account.id.clone() },
            Record::GithubSignIn(sign_in) => RecordKey::GithubSignIn { id: sign_in.id.clone() },
            Record::GithubApp(app) => RecordKey::GithubApp { id: app.id },
            Record::ToolServer(server) => RecordKey::ToolServer { id: server.id.clone() },
            Record::Terminal(terminal) => RecordKey::Terminal { id: terminal.id.clone() },
            Record::ModelList(list) => RecordKey::ModelList(list.key.clone()),
            Record::HubUpdate(_) => RecordKey::HubUpdate,
            Record::Proposal(proposal) => RecordKey::Proposal { id: proposal.id.clone() },
            Record::ReplyHeard(heard) => {
                RecordKey::ReplyHeard { session_id: heard.session_id.clone(), message: heard.message.clone() }
            }
        }
    }

    /// The revision of this copy, which says whether it replaces another: one at a higher revision does.
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
            Record::GithubAccount(account) => account.rev,
            Record::GithubSignIn(sign_in) => sign_in.rev,
            Record::GithubApp(app) => app.rev,
            Record::ToolServer(server) => server.rev,
            Record::Terminal(terminal) => terminal.rev,
            Record::ModelList(list) => list.rev,
            Record::HubUpdate(update) => update.rev,
            Record::Proposal(proposal) => proposal.rev,
            Record::ReplyHeard(heard) => heard.rev,
        }
    }
}

/// Every record a hub holds, read at one moment, but for subagents and proposals, which are listed by session, and the
/// observations it sends at its own level.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Records {
    pub repos: Vec<Repo>,
    pub workspaces: Vec<Workspace>,
    pub sessions: Vec<Session>,
    pub comments: Vec<DiffComment>,
    /// `None` leaves a client with no settings to show until the feed tells it of them.
    #[serde(default)]
    pub hub_settings: Option<HubSettings>,
    /// Empty where the hub keeps none.
    #[serde(default)]
    pub accounts: Vec<Account>,
    /// `None` leaves a client with nothing in the new-workspace dialog until the feed tells it of what is.
    #[serde(default)]
    pub new_workspace_draft: Option<NewWorkspaceDraft>,
    /// Empty where the hub keeps none.
    #[serde(default)]
    pub github_accounts: Vec<GithubAccount>,
    /// Empty where the hub has none.
    #[serde(default)]
    pub github_sign_ins: Vec<GithubSignIn>,
    /// Empty where the hub has no app of your own.
    #[serde(default)]
    pub github_apps: Vec<GithubApp>,
    /// The tool servers the hub has of its own. Empty where it has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_servers: Vec<ToolServer>,
    /// The terminals open in every workspace. Empty when none is open.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminals: Vec<Terminal>,
    /// What the agents that discover their own models offer, where the hub has been asked. Empty while it keeps none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_lists: Vec<ModelList>,
    /// The release the hub would update to, from a hub installed as a service, once it has read the feed, as
    /// [`HubUpdate`] says. `None` from any other hub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_update: Option<HubUpdate>,
    /// The observations the hub sends at its own level, as [`ObservationKey::level`](super::ObservationKey::level)
    /// says, under the list's own `rev`, as its records are. One of another level isn't here, so the list doesn't say
    /// it's gone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<Observation>,
    /// The hub's newest revision when this was read. A record missing from the list is gone, unless a client holds a
    /// copy newer than this, and so is an observation of the hub's level.
    pub rev: Rev,
}

impl Records {
    /// Each record in the list, of every kind. It leaves out the list's observations, which aren't records, and its own
    /// revision, `rev`.
    pub fn into_records(self) -> impl Iterator<Item = Record> {
        let Records {
            repos,
            workspaces,
            sessions,
            comments,
            hub_settings,
            accounts,
            new_workspace_draft,
            github_accounts,
            github_sign_ins,
            github_apps,
            tool_servers,
            terminals,
            model_lists,
            hub_update,
            observations: _,
            rev: _,
        } = self;
        let records = repos.into_iter().map(Record::Repo);
        let records = records.chain(workspaces.into_iter().map(Record::Workspace));
        let records = records.chain(sessions.into_iter().map(Record::Session));
        let records = records.chain(comments.into_iter().map(Record::Comment));
        let records = records.chain(hub_settings.into_iter().map(Record::HubSettings));
        let records = records.chain(accounts.into_iter().map(Record::Account));
        let records = records.chain(new_workspace_draft.into_iter().map(Record::NewWorkspaceDraft));
        let records = records.chain(github_accounts.into_iter().map(Record::GithubAccount));
        let records = records.chain(github_sign_ins.into_iter().map(Record::GithubSignIn));
        let records = records.chain(github_apps.into_iter().map(Record::GithubApp));
        let records = records.chain(tool_servers.into_iter().map(Record::ToolServer));
        let records = records.chain(terminals.into_iter().map(Record::Terminal));
        let records = records.chain(model_lists.into_iter().map(Record::ModelList));
        records.chain(hub_update.into_iter().map(Record::HubUpdate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn a_list_gives_its_github_accounts_sign_ins_and_apps_and_its_tool_servers_as_records() {
        let list = Records {
            github_accounts: vec![fixtures::github_account("g")],
            github_sign_ins: vec![fixtures::github_sign_in("s")],
            github_apps: vec![fixtures::github_app(9)],
            tool_servers: vec![fixtures::tool_server("t")],
            ..Records::default()
        };

        let keys: Vec<RecordKey> = list.into_records().map(|record| record.key()).collect();

        let signing_in = RecordKey::GithubSignIn { id: "s".into() };
        let app = RecordKey::GithubApp { id: GithubAppId::new(9) };
        let server = RecordKey::ToolServer { id: "t".into() };
        assert_eq!(keys, [RecordKey::GithubAccount { id: "g".into() }, signing_in, app, server]);
    }
}
