//! Each field of a record that an observation replaced, blanked as a client takes the record in. A client of this
//! version reads each such value from its observation, but a hub that speaks version 4 too fills the field on every
//! record it sends, whatever the version its connection chose, for clients of version 4. Blanked, a view that still read
//! one would read nothing, which a test of every screen catches, rather than a value as of another moment than the
//! observation's.

use protocol::model::{
    Account, GithubAccount, GithubApp, GithubUser, GithubUserId, HubUpdate, ModelList, Record, Session, Wait, Workspace,
};

/// `record`, with each field an observation replaced blanked.
pub(super) fn blanked(mut record: Record) -> Record {
    match &mut record {
        Record::Workspace(workspace) => blank_workspace(workspace),
        Record::Session(session) => blank_session(session),
        Record::Account(account) => blank_account(account),
        Record::GithubAccount(account) => blank_github_account(account),
        Record::GithubApp(app) => blank_app(app),
        Record::ModelList(ModelList { catalog, .. }) => *catalog = None,
        Record::HubUpdate(HubUpdate { version, .. }) => *version = None,
        // None of theirs is an observation's.
        Record::Repo(_)
        | Record::Comment(_)
        | Record::HubSettings(_)
        | Record::NewWorkspaceDraft(_)
        | Record::Subagent(_)
        | Record::GithubSignIn(_)
        | Record::FoundGithubLogin(_)
        | Record::GithubReach(_)
        | Record::Terminal(_)
        | Record::Proposal(_)
        | Record::ReplyHeard(_) => {}
    }
    record
}

/// Its branches and which build its worker is, and how many files were uncommitted as its pull request was read.
fn blank_workspace(workspace: &mut Workspace) {
    (workspace.branches, workspace.worker_build) = (None, None);
    if let Some(link) = workspace.pull_request_link.as_mut().and_then(|linked| linked.link.as_mut()) {
        link.uncommitted = 0;
    }
}

/// How full its agent's context is, its to-do list, what the API's cache holds of it, and what each open wait last
/// read. An ended wait keeps its last read on its record.
fn blank_session(session: &mut Session) {
    (session.context, session.todos, session.prompt_cache) = (None, None, None);
    for wait in session.waits.iter_mut().flatten().chain(session.wait.as_mut()) {
        blank_wait(wait);
    }
}

fn blank_wait(wait: &mut Wait) {
    if wait.ended.is_some() {
        return;
    }
    if let Some(watch) = wait.watch.as_mut() {
        **watch = watch.recorded();
    }
}

/// Who it is, and what's left of its limits.
fn blank_account(account: &mut Account) {
    (account.model_provider, account.plan, account.login, account.email) = (None, None, None, None);
    account.limits.clear();
}

/// What GitHub calls its user, and what its installation, if it's one, is on. Each keeps its user's number, which
/// links the record to GitHub.
fn blank_github_account(account: &mut GithubAccount) {
    account.user.name = "".into();
    if let Some(installation) = account.installation.as_mut() {
        installation.on.name = "".into();
    }
}

/// What GitHub says of it, and its bot.
fn blank_app(app: &mut GithubApp) {
    (app.slug, app.name) = ("".into(), String::new());
    app.owner = GithubUser { id: GithubUserId::new(0), name: "".into() };
    app.bot = None;
}

#[cfg(test)]
mod tests {
    use protocol::capability::settings::ModelCatalog;
    use protocol::model::{
        Account, AgentKind, ContextUsage, GithubAccount, ModelList, ModelListKey, ModelLookup, Record, RecordKey,
        Records, Rev, Session, Workspace,
    };

    use crate::records::HubRecords;

    /// Each record taken in, from a list or from the feed, has every field an observation replaced blank, whatever
    /// the hub filled it with.
    #[test]
    fn each_field_an_observation_replaced_is_blank_once_taken_in() {
        let filled = "filled".to_string();
        let context =
            ContextUsage { used_tokens: 1, max_tokens: 2, model_tokens: None, max_set_by: None, parts: Vec::new() };
        let workspace = Workspace { worker_build: Some(filled.clone()), ..protocol::fixtures::workspace("w") };
        let session = Session { context: Some(context), ..protocol::fixtures::session("s") };
        let account =
            Account { plan: Some(filled.clone()), email: Some(filled.clone()), ..protocol::fixtures::account("a") };
        let mut github = protocol::fixtures::github_account("g");
        github.user.name = filled.as_str().into();
        let key = ModelListKey {
            agent: AgentKind::Codex,
            repo: "r".into(),
            provider: "local".into(),
            account: None,
            cli_version: None,
        };
        let list = ModelList {
            key: key.clone(),
            catalog: Some(ModelCatalog::default()),
            lookup: ModelLookup::Answered,
            rev: Rev::new(4),
        };
        let mut held = HubRecords::default();

        held.apply_list(Records {
            workspaces: vec![workspace],
            sessions: vec![session],
            accounts: vec![account],
            rev: Rev::new(2),
            ..Records::default()
        });
        held.apply_record(Record::GithubAccount(GithubAccount { rev: Rev::new(3), ..github }));
        held.apply_record(Record::ModelList(list));

        let shown = held.snapshot();
        assert_eq!(shown.workspace(&"w".into()).unwrap().worker_build, None);
        assert_eq!(shown.session(&"s".into()).unwrap().context, None);
        let account = shown.account(&"a".into()).unwrap();
        assert_eq!((&account.plan, &account.email), (&None, &None));
        assert_eq!(shown.github_account(&"g".into()).unwrap().user.name.as_str(), "");
        let Some(Record::ModelList(list)) = held.records.held.get(&RecordKey::ModelList(key)).map(|kept| &*kept.copy)
        else {
            panic!("the list held")
        };
        assert_eq!(list.catalog, None);
    }
}
