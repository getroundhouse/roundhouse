//! What a hub sends of each kind of record that once held a field a client of version 5 reads elsewhere or not at all,
//! spelled out as these types write it, to the byte, since that client reads each from a hub that leaves the fields
//! out: a field an observation replaced, which it reads from its observation, a repository's `path`, which it reads
//! from `origin`, a chat's `wait`, which it reads from `waits`, and the list of every record's `found_github_logins`,
//! which it reads as empty. And what the hub of the build before this one sends of each, with the fields there, which
//! these types read. Every kind of record that held such a field is here.

use protocol::fixtures;
use protocol::model::{
    Account, GithubAccount, GithubApp, GithubAppId, GithubInstallation, GithubInstallationId, GithubRepositoryName,
    HubUpdate, IssueNumber, ModelList, ModelListKey, ModelLookup, PullRequestLink, PullRequestLinked, Records, Repo,
    RepoOrigin, RepoUrl, Rev, Session, Wait, WaitEnd, WaitEnded, Workspace,
};

use crate::keeps_its_form;

/// A repository with no checkout, where it is said by `origin` alone.
const REPO: &str = concat!(
    r#"{"id":"r","name":"repo","origin":{"kind":"remote","url":"https://github.com/ada/roundhouse.git"},"#,
    r#""created_at_ms":1,"rev":2}"#,
);

/// A workspace whose pull request is #7, with none of its branches, which build its worker is, or how many files were
/// uncommitted as its pull request was read.
const WORKSPACE: &str = concat!(
    r#"{"id":"w","repo_id":"r","name":"w","branch":"w","base_ref":null,"path":null,"status":"ready","#,
    r#""provider":"local","instance_id":"w","created_at_ms":0,"archived_at_ms":null,"user_active_at_ms":null,"#,
    r#""setup":{"state":"succeeded"},"port_base":55000,"rev":3,"title":null,"title_source":"first_line","#,
    r#""pull_request_link":{"repository":"o/r","number":7,"url":"https://github.com/o/r/pull/7"}}"#,
);

/// A chat with an open wait and an ended one, with none of how full its context is, its cache, its to-do list or its
/// oldest open wait apart from the rest. The open wait's watch keeps its keys, empty, and the ended wait keeps what it
/// last read.
const SESSION: &str = concat!(
    r#"{"id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","#,
    r#""beyond":"asks"},"effort":"xhigh","status":"idle","last_seq":0,"created_at_ms":0,"updated_at_ms":0,"#,
    r#""title":null,"closed_at_ms":null,"reply_seq":0,"turn_started_at_ms":null,"user_active_at_ms":null,"#,
    r#""read_seq":0,"background_tasks":[],"waits":[{"id":"open","session_id":"s","workspace_id":"w","#,
    r#""number":7,"call":"open","until":"required_checks","started_at_ms":1,"watching":{"requested":[7],"#,
    r#""snapshot":{"repository":{"url":"https://github.com/example/project","name_with_owner":"example/project"},"#,
    r#""observed_at_ms":1,"pull_requests":[]},"baseline":[],"deadline_at_ms":null,"read_error":null,"#,
    r#""last_attempt_at_ms":1,"affected":[]}},{"id":"ended","session_id":"s","workspace_id":"w","number":7,"#,
    r#""call":"ended","until":"required_checks","started_at_ms":1,"ended":{"at_ms":2,"how":"checks_passed"},"#,
    r#""watching":{"requested":[7],"snapshot":{"repository":{"url":"https://github.com/example/project","#,
    r#""name_with_owner":"example/project"},"observed_at_ms":1,"pull_requests":[{"repository":"example/project","#,
    r#""number":7,"summary":{"said":{"about":{"url":"https://github.com/example/project/pull/7","title":"Change 7","#,
    r#""opened_at_ms":1,"comments":0,"labels":[],"body":""},"state":"OPEN","is_draft":false,"review":"","#,
    r#""merge_state":"CLEAN","mergeable":"mergeable","base_branch":"main","head":"head-7","base":"base","#,
    r#""added":0,"removed":0},"read_at_ms":1},"history":{"said":{},"read_at_ms":1},"stack":{"said":{"destination":"main","#,
    r#""dependencies":[],"linear":true},"read_at_ms":1},"checks":{"said":{"revision":"head-7","requirements_known":true,"#,
    r#""checks":[{"name":"checks","source":"app:1","url":null,"state":"pending","required":true}]},"#,
    r#""read_at_ms":1}}]},"baseline":[],"deadline_at_ms":null,"read_error":null,"last_attempt_at_ms":1,"#,
    r#""affected":[]}}],"account":null,"queued":[],"queue_held":false,"open_approvals":[],"rev":4,"#,
    r#""title_source":"first_line"}"#,
);

/// An account the person named, with none of who it is or what's left of its limits.
const ACCOUNT: &str = r#"{"id":"a","name":"Work","login_agent":null,"held_login":null,"used_at_ms":null,"rev":5}"#;

/// A GitHub account that's an installation of an app of your own, which names GitHub's users by their numbers alone.
const GITHUB_ACCOUNT: &str = concat!(
    r#"{"id":"g","user":{"id":7},"held_login":{"how":"token","connected_at_ms":0,"ends_at_ms":null,"#,
    r#""needs_sign_in":false},"installation":{"app":9,"id":70,"on":{"id":2}},"rev":6}"#,
);

/// An app of your own, with none of what GitHub says of it or its bot.
const GITHUB_APP: &str = r#"{"id":9,"rev":7}"#;

/// A list of Codex's models, with none of what Codex offers.
const MODEL_LIST: &str = concat!(
    r#"{"agent":"codex","repo":"r","provider":"local","account":null,"lookup":{"state":"answered"},"#,
    r#""rev":8}"#,
);

/// What a hub found in the update feed, with none of the release offered.
const HUB_UPDATE: &str = r#"{"looked_at_ms":1795230000000,"problem":null,"rev":9}"#;

/// A list of every record, with the workspace above, and no `github_reach` or `found_github_logins`.
const RECORDS: &str = concat!(
    r#"{"repos":[],"workspaces":[{"id":"w","repo_id":"r","name":"w","branch":"w","base_ref":null,"path":null,"#,
    r#""status":"ready","provider":"local","instance_id":"w","created_at_ms":0,"archived_at_ms":null,"#,
    r#""user_active_at_ms":null,"setup":{"state":"succeeded"},"port_base":55000,"rev":3,"title":null,"#,
    r#""title_source":"first_line","pull_request_link":{"repository":"o/r","number":7,"url":"https://github.com/o/r/pull/7"}}],"#,
    r#""sessions":[],"comments":[],"hub_settings":null,"accounts":[],"new_workspace_draft":null,"github_accounts":[],"#,
    r#""github_sign_ins":[],"github_apps":[],"rev":10}"#,
);

// As the hub of the build before sends each, with every such field filled from its observation for a client of
// version 4, written out by that build's types.

/// A repository with no checkout, with `""` as its checkout beside `origin`.
const REPO_FILLED: &str = concat!(
    r#"{"id":"r","name":"repo","path":"","origin":{"kind":"remote","#,
    r#""url":"https://github.com/ada/roundhouse.git"},"created_at_ms":1,"rev":2}"#,
);

/// A workspace, with its branches, its worker's build and how many files are uncommitted on its link.
const WORKSPACE_FILLED: &str = concat!(
    r#"{"id":"w","repo_id":"r","name":"w","branch":"w","base_ref":null,"path":null,"status":"ready","#,
    r#""provider":"local","instance_id":"w","created_at_ms":0,"archived_at_ms":null,"#,
    r#""user_active_at_ms":null,"setup":{"state":"succeeded"},"port_base":55000,"rev":3,"title":null,"#,
    r#""title_source":"first_line","pull_request_link":{"repository":"o/r","number":7,"#,
    r#""url":"https://github.com/o/r/pull/7","uncommitted":2},"branches":{"head":{"kind":"branch","#,
    r#""name":"w"},"had":[]},"worker_build":"0.1.900"}"#,
);

/// A chat with its context, its cache and its to-do list, and an open wait whose watch holds what it last read, which
/// is its oldest open wait too, as `wait`.
const SESSION_FILLED: &str = concat!(
    r#"{"id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","#,
    r#""beyond":"asks"},"effort":"xhigh","status":"idle","last_seq":0,"created_at_ms":0,"updated_at_ms":0,"#,
    r#""title":null,"closed_at_ms":null,"reply_seq":0,"turn_started_at_ms":null,"user_active_at_ms":null,"#,
    r#""read_seq":0,"background_tasks":[],"#,
    r#""wait":{"id":"open","session_id":"s","workspace_id":"w","number":7,"call":"open","until":"required_checks","#,
    r#""started_at_ms":1,"watching":{"requested":[7],"#,
    r#""snapshot":{"repository":{"url":"https://github.com/example/project","name_with_owner":"example/project"},"#,
    r#""observed_at_ms":1,"pull_requests":[{"repository":"example/project","number":7,"#,
    r#""summary":{"said":{"about":{"url":"https://github.com/example/project/pull/7","title":"Change 7","#,
    r#""opened_at_ms":1,"comments":0,"labels":[],"body":""},"state":"OPEN","is_draft":false,"review":"","#,
    r#""merge_state":"CLEAN","mergeable":"mergeable","base_branch":"main","head":"head-7","base":"base","added":0,"#,
    r#""removed":0},"read_at_ms":1},"history":{"said":{},"read_at_ms":1},"stack":{"said":{"destination":"main","#,
    r#""dependencies":[],"linear":true},"read_at_ms":1},"checks":{"said":{"revision":"head-7","#,
    r#""requirements_known":true,"checks":[{"name":"checks","source":"app:1","url":null,"state":"pending","#,
    r#""required":true}]},"read_at_ms":1}}]},"baseline":[],"deadline_at_ms":null,"read_error":null,"#,
    r#""last_attempt_at_ms":1,"affected":[]}},"#,
    r#""waits":[{"id":"open","session_id":"s","workspace_id":"w","#,
    r#""number":7,"call":"open","until":"required_checks","started_at_ms":1,"watching":{"requested":[7],"#,
    r#""snapshot":{"repository":{"url":"https://github.com/example/project","#,
    r#""name_with_owner":"example/project"},"observed_at_ms":1,"#,
    r#""pull_requests":[{"repository":"example/project","number":7,"#,
    r#""summary":{"said":{"about":{"url":"https://github.com/example/project/pull/7","title":"Change 7","#,
    r#""opened_at_ms":1,"comments":0,"labels":[],"body":""},"state":"OPEN","is_draft":false,"review":"","#,
    r#""merge_state":"CLEAN","mergeable":"mergeable","base_branch":"main","head":"head-7","base":"base","#,
    r#""added":0,"removed":0},"read_at_ms":1},"history":{"said":{},"read_at_ms":1},"#,
    r#""stack":{"said":{"destination":"main","dependencies":[],"linear":true},"read_at_ms":1},"#,
    r#""checks":{"said":{"revision":"head-7","requirements_known":true,"checks":[{"name":"checks","#,
    r#""source":"app:1","url":null,"state":"pending","required":true}]},"read_at_ms":1}}]},"baseline":[],"#,
    r#""deadline_at_ms":null,"read_error":null,"last_attempt_at_ms":1,"affected":[]}},{"id":"ended","#,
    r#""session_id":"s","workspace_id":"w","number":7,"call":"ended","until":"required_checks","#,
    r#""started_at_ms":1,"ended":{"at_ms":2,"how":"checks_passed"},"watching":{"requested":[7],"#,
    r#""snapshot":{"repository":{"url":"https://github.com/example/project","#,
    r#""name_with_owner":"example/project"},"observed_at_ms":1,"#,
    r#""pull_requests":[{"repository":"example/project","number":7,"#,
    r#""summary":{"said":{"about":{"url":"https://github.com/example/project/pull/7","title":"Change 7","#,
    r#""opened_at_ms":1,"comments":0,"labels":[],"body":""},"state":"OPEN","is_draft":false,"review":"","#,
    r#""merge_state":"CLEAN","mergeable":"mergeable","base_branch":"main","head":"head-7","base":"base","#,
    r#""added":0,"removed":0},"read_at_ms":1},"history":{"said":{},"read_at_ms":1},"#,
    r#""stack":{"said":{"destination":"main","dependencies":[],"linear":true},"read_at_ms":1},"#,
    r#""checks":{"said":{"revision":"head-7","requirements_known":true,"checks":[{"name":"checks","#,
    r#""source":"app:1","url":null,"state":"pending","required":true}]},"read_at_ms":1}}]},"baseline":[],"#,
    r#""deadline_at_ms":null,"read_error":null,"last_attempt_at_ms":1,"affected":[]}}],"#,
    r#""context":{"used_tokens":5,"max_tokens":10,"parts":[]},"prompt_cache":{"used_at_ms":1,"#,
    r#""ttl":"five_minutes","model":null,"tokens":3},"todos":{"items":[{"text":"Test","status":"pending"}]},"#,
    r#""account":null,"queued":[],"queue_held":false,"open_approvals":[],"rev":4,"title_source":"first_line"}"#,
);

/// An account, with who it is and what's left of its limits.
const ACCOUNT_FILLED: &str = concat!(
    r#"{"id":"a","model_provider":"anthropic","plan":"max","login":"claude.ai","email":"ada@example.com","#,
    r#""name":"Work","login_agent":null,"limits":[{"window":"five_hours","used_percent":40,"resets_at_ms":1,"#,
    r#""heard_at_ms":null}],"held_login":null,"used_at_ms":null,"rev":5}"#,
);

/// A GitHub account that's an installation of an app of your own, naming its user and what it's on.
const GITHUB_ACCOUNT_FILLED: &str = concat!(
    r#"{"id":"g","user":{"id":7,"name":"ada"},"held_login":{"how":"token","connected_at_ms":0,"#,
    r#""ends_at_ms":null,"needs_sign_in":false},"installation":{"app":9,"id":70,"on":{"id":2,"#,
    r#""name":"example"}},"rev":6}"#,
);

/// An app of your own, with what GitHub says of it and its bot.
const GITHUB_APP_FILLED: &str = concat!(
    r#"{"id":9,"slug":"ada-roundhouse","name":"Ada's Roundhouse","owner":{"id":1,"name":"ada"},"#,
    r#""bot":{"id":41,"name":"ada-roundhouse[bot]"},"rev":7}"#,
);

/// A list of Codex's models, with what Codex offers.
const MODEL_LIST_FILLED: &str = concat!(
    r#"{"agent":"codex","repo":"r","provider":"local","account":null,"catalog":{"offered":[],"#,
    r#""default_model":"gpt-5"},"lookup":{"state":"answered"},"rev":8}"#,
);

/// What a hub found in the update feed, with the release offered.
const HUB_UPDATE_FILLED: &str = r#"{"version":"0.1.900","looked_at_ms":1795230000000,"problem":null,"rev":9}"#;

/// A list of every record, with the workspace above and a repository's reach.
const RECORDS_FILLED: &str = concat!(
    r#"{"repos":[],"workspaces":[{"id":"w","repo_id":"r","name":"w","branch":"w","base_ref":null,"#,
    r#""path":null,"status":"ready","provider":"local","instance_id":"w","created_at_ms":0,"#,
    r#""archived_at_ms":null,"user_active_at_ms":null,"setup":{"state":"succeeded"},"port_base":55000,"#,
    r#""rev":3,"title":null,"title_source":"first_line","pull_request_link":{"repository":"o/r","number":7,"#,
    r#""url":"https://github.com/o/r/pull/7","uncommitted":2},"branches":{"head":{"kind":"branch","#,
    r#""name":"w"},"had":[]},"worker_build":"0.1.900"}],"sessions":[],"comments":[],"hub_settings":null,"#,
    r#""accounts":[],"new_workspace_draft":null,"github_accounts":[],"github_sign_ins":[],"#,
    r#""found_github_logins":[],"github_apps":[],"github_reach":[{"repo":"r","reach":{"state":"pushes","#,
    r#""repository":"example/project","username":"ada"},"answered_at_ms":0,"stale":false,"rev":2}],"#,
    r#""rev":10}"#,
);

fn repo() -> Repo {
    let url = RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap();
    Repo { id: "r".into(), name: "repo".into(), origin: RepoOrigin::Remote { url }, created_at_ms: 1, rev: Rev::new(2) }
}

fn workspace() -> Workspace {
    let link = PullRequestLink {
        repository: GithubRepositoryName::named("o/r"),
        number: IssueNumber::try_from(7).unwrap(),
        url: "https://github.com/o/r/pull/7".into(),
    };
    Workspace {
        pull_request_link: Some(PullRequestLinked { link: Some(link) }),
        rev: Rev::new(3),
        ..fixtures::workspace("w")
    }
}

fn session() -> Session {
    let mut open = fixtures::pull_request_wait("open");
    if let Some(watch) = open.watch.as_mut() {
        **watch = watch.recorded();
    }
    session_waiting(open)
}

/// The chat above, with `open` as its open wait.
fn session_waiting(open: Wait) -> Session {
    let ended = Wait {
        id: "ended".into(),
        call: Some("ended".into()),
        ended: Some(WaitEnded { at_ms: 2, how: WaitEnd::ChecksPassed }),
        ..fixtures::pull_request_wait("ended")
    };
    Session { waits: vec![open, ended], rev: Rev::new(4), ..fixtures::session("s") }
}

fn account() -> Account {
    Account { name: Some("Work".into()), rev: Rev::new(5), ..fixtures::account("a") }
}

fn github_account() -> GithubAccount {
    let on = fixtures::github_user(2);
    let installation = GithubInstallation { app: GithubAppId::new(9), id: GithubInstallationId::new(70), on };
    let user = fixtures::github_user(7);
    GithubAccount { user, installation: Some(installation), rev: Rev::new(6), ..fixtures::github_account("g") }
}

fn github_app() -> GithubApp {
    GithubApp { rev: Rev::new(7), ..fixtures::github_app(9) }
}

fn model_list() -> ModelList {
    let key = ModelListKey {
        agent: protocol::model::AgentKind::Codex,
        repo: "r".into(),
        provider: "local".into(),
        account: None,
        cli_version: None,
    };
    ModelList { key, lookup: ModelLookup::Answered, rev: Rev::new(8) }
}

fn hub_update() -> HubUpdate {
    HubUpdate { looked_at_ms: 1_795_230_000_000, problem: None, attempt: None, rev: Rev::new(9) }
}

fn records() -> Records {
    Records { workspaces: vec![workspace()], rev: Rev::new(10), ..Records::default() }
}

/// Each is written to the byte as a client of version 5 reads it, and reads as it was.
#[test]
fn a_record_keeps_the_form_a_client_of_version_5_reads() {
    keeps_its_form(&repo(), REPO);
    keeps_its_form(&workspace(), WORKSPACE);
    keeps_its_form(&session(), SESSION);
    keeps_its_form(&account(), ACCOUNT);
    keeps_its_form(&github_account(), GITHUB_ACCOUNT);
    keeps_its_form(&github_app(), GITHUB_APP);
    keeps_its_form(&model_list(), MODEL_LIST);
    keeps_its_form(&hub_update(), HUB_UPDATE);
    keeps_its_form(&records(), RECORDS);
}

/// Each, as the hub of the build before sends it, filled, reads as it does without the fields, since these types skip a
/// field they don't know, so a client of this build can follow that hub, which speaks version 6 too. All but the open
/// wait's watch, which that hub fills with what the wait last read, and which a client reads from its observation.
#[test]
fn a_record_the_hub_of_the_build_before_fills_reads_as_it_does_without_the_fields() {
    assert_eq!(serde_json::from_str::<Repo>(REPO_FILLED).unwrap(), repo());
    assert_eq!(serde_json::from_str::<Workspace>(WORKSPACE_FILLED).unwrap(), workspace());
    assert_eq!(
        serde_json::from_str::<Session>(SESSION_FILLED).unwrap(),
        session_waiting(fixtures::pull_request_wait("open"))
    );
    assert_eq!(serde_json::from_str::<Account>(ACCOUNT_FILLED).unwrap(), account());
    assert_eq!(serde_json::from_str::<GithubAccount>(GITHUB_ACCOUNT_FILLED).unwrap(), github_account());
    assert_eq!(serde_json::from_str::<GithubApp>(GITHUB_APP_FILLED).unwrap(), github_app());
    assert_eq!(serde_json::from_str::<ModelList>(MODEL_LIST_FILLED).unwrap(), model_list());
    assert_eq!(serde_json::from_str::<HubUpdate>(HUB_UPDATE_FILLED).unwrap(), hub_update());
    assert_eq!(serde_json::from_str::<Records>(RECORDS_FILLED).unwrap(), records());
}
