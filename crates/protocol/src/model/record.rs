//! What a record is, of any kind: which one it is, its revision, and every record a hub holds, read at one moment. A
//! new kind of record is added here, and the compiler lists what else has to name it, so nothing that keeps records
//! has to list their kinds.

use serde::{Deserialize, Serialize};

use super::{
    Account, AccountId, CommentId, DiffComment, FoundGithubLogin, GithubAccount, GithubAccountId, HubSettings,
    NewWorkspaceDraft, ProviderName, Repo, RepoId, Rev, Session, SessionId, Subagent, ToolUseId, Workspace,
    WorkspaceId,
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
    /// Removed as the account is disconnected, as [`GithubAccount`] says.
    GithubAccount {
        id: GithubAccountId,
    },
    /// A provider has at most one, once it's been asked who its machines' `gh` is logged in as. One with none hasn't been,
    /// or its worker was too old to say.
    FoundGithubLogin {
        provider: ProviderName,
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
    FoundGithubLogin(FoundGithubLogin),
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
            Record::GithubAccount(account) => RecordKey::GithubAccount { id: account.id.clone() },
            Record::FoundGithubLogin(found) => RecordKey::FoundGithubLogin { provider: found.provider.clone() },
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
            Record::GithubAccount(account) => account.rev,
            Record::FoundGithubLogin(found) => found.rev,
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
    /// Empty from a hub that doesn't keep GitHub accounts, as its settings' `github` says.
    #[serde(default)]
    pub github_accounts: Vec<GithubAccount>,
    /// Empty from a hub that doesn't keep GitHub accounts, and until a provider has been asked.
    #[serde(default)]
    pub found_github_logins: Vec<FoundGithubLogin>,
    /// The hub's newest revision when this was read. A record missing from the list is gone, unless
    /// a client holds a copy newer than this.
    pub rev: Rev,
}

impl Records {
    /// Each record in the list, of every kind. The list's own revision is `rev`, which this leaves out.
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
            found_github_logins,
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
        records.chain(found_github_logins.into_iter().map(Record::FoundGithubLogin))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::fixtures;

    #[test]
    fn a_list_gives_its_github_accounts_and_found_logins_as_records() {
        let list = Records {
            github_accounts: vec![fixtures::github_account("g")],
            found_github_logins: vec![fixtures::found_github_login("local")],
            ..Records::default()
        };

        let keys: Vec<RecordKey> = list.into_records().map(|record| record.key()).collect();

        let found = RecordKey::FoundGithubLogin { provider: "local".into() };
        assert_eq!(keys, [RecordKey::GithubAccount { id: "g".into() }, found]);
    }

    #[test]
    fn a_list_from_a_hub_that_keeps_no_github_accounts_reads_with_none() {
        let older = json!({"repos": [], "workspaces": [], "sessions": [], "comments": [], "rev": 3});

        let list: Records = serde_json::from_value(older).unwrap();

        assert_eq!((list.github_accounts, list.found_github_logins), (vec![], vec![]));
    }
}
