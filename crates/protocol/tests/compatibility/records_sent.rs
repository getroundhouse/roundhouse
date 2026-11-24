//! What a hub sends of each kind of record that once held a field an observation replaced, with none of those fields,
//! spelled out, since a client of version 5 reads each such field from its observation, and reads these from a hub that
//! leaves the fields out. Every kind of record that holds such a field is here.

use protocol::fixtures;
use protocol::model::{
    Account, GithubAccount, GithubApp, GithubAppId, GithubInstallation, GithubInstallationId, GithubRepositoryName,
    GithubUser, GithubUserId, HubUpdate, IssueNumber, ModelList, ModelListKey, ModelLookup, PullRequestLink,
    PullRequestLinked, Records, Rev, Session, Wait, WaitEnd, WaitEnded, Workspace,
};
use serde::de::DeserializeOwned;

/// A workspace whose pull request is #7, with none of its branches, which build its worker is, or how many files were
/// uncommitted as its pull request was read.
const WORKSPACE: &str = concat!(
    r#"{"id":"w","repo_id":"r","name":"w","branch":"w","base_ref":null,"path":null,"status":"ready","#,
    r#""provider":"local","instance_id":"w","created_at_ms":0,"archived_at_ms":null,"user_active_at_ms":null,"#,
    r#""setup":{"state":"succeeded"},"port_base":55000,"rev":3,"title":null,"title_source":"first_line","#,
    r#""pull_request_link":{"repository":"o/r","number":7,"url":"https://github.com/o/r/pull/7"}}"#,
);

/// A chat with an open wait and an ended one, with none of how full its context is, its cache or its to-do list. The
/// open wait's watch keeps its keys, empty, and the ended wait keeps what it last read.
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
    r#""affected":[]}}],"account":null,"queued":[],"queue_held":false,"rev":4,"title_source":"first_line"}"#,
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

/// A list of every record, with the workspace above, and no `github_reach`.
const RECORDS: &str = concat!(
    r#"{"repos":[],"workspaces":[{"id":"w","repo_id":"r","name":"w","branch":"w","base_ref":null,"path":null,"#,
    r#""status":"ready","provider":"local","instance_id":"w","created_at_ms":0,"archived_at_ms":null,"#,
    r#""user_active_at_ms":null,"setup":{"state":"succeeded"},"port_base":55000,"rev":3,"title":null,"#,
    r#""title_source":"first_line","pull_request_link":{"repository":"o/r","number":7,"url":"https://github.com/o/r/pull/7"}}],"#,
    r#""sessions":[],"comments":[],"hub_settings":null,"accounts":[],"new_workspace_draft":null,"github_accounts":[],"#,
    r#""github_sign_ins":[],"found_github_logins":[],"github_apps":[],"rev":10}"#,
);

fn workspace() -> Workspace {
    let link = PullRequestLink {
        repository: GithubRepositoryName::named("o/r"),
        number: IssueNumber::try_from(7).unwrap(),
        url: "https://github.com/o/r/pull/7".into(),
        uncommitted: 0,
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
    let ended = Wait {
        id: "ended".into(),
        call: Some("ended".into()),
        ended: Some(WaitEnded { at_ms: 2, how: WaitEnd::ChecksPassed }),
        ..fixtures::pull_request_wait("ended")
    };
    Session { waits: Some(vec![open, ended]), rev: Rev::new(4), ..fixtures::session("s") }
}

fn account() -> Account {
    Account { name: Some("Work".into()), rev: Rev::new(5), ..fixtures::account("a") }
}

/// GitHub's user `number`, as a record that names it by its number holds it.
fn user(number: u64) -> GithubUser {
    GithubUser { id: GithubUserId::new(number), name: "".into() }
}

fn github_account() -> GithubAccount {
    let installation = GithubInstallation { app: GithubAppId::new(9), id: GithubInstallationId::new(70), on: user(2) };
    GithubAccount { user: user(7), installation: Some(installation), rev: Rev::new(6), ..fixtures::github_account("g") }
}

fn github_app() -> GithubApp {
    let app = fixtures::github_app(9);
    GithubApp { slug: "".into(), name: String::new(), owner: GithubUser::nobody(), bot: None, rev: Rev::new(7), ..app }
}

fn model_list() -> ModelList {
    let key = ModelListKey {
        agent: protocol::model::AgentKind::Codex,
        repo: "r".into(),
        provider: "local".into(),
        account: None,
        cli_version: None,
    };
    ModelList { key, catalog: None, lookup: ModelLookup::Answered, rev: Rev::new(8) }
}

fn hub_update() -> HubUpdate {
    HubUpdate { version: None, looked_at_ms: 1_795_230_000_000, problem: None, attempt: None, rev: Rev::new(9) }
}

fn records() -> Records {
    Records { workspaces: vec![workspace()], rev: Rev::new(10), ..Records::default() }
}

/// What `line` reads as.
fn read<T: DeserializeOwned>(line: &str) -> T {
    serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"))
}

/// Each reads as this build reads the record with every such field blank, as it blanks them.
#[test]
fn a_record_without_what_an_observation_replaced_reads_as_this_build_reads_it_blank() {
    assert_eq!(read::<Workspace>(WORKSPACE), workspace());
    assert_eq!(read::<Session>(SESSION), session());
    assert_eq!(read::<Account>(ACCOUNT), account());
    assert_eq!(read::<GithubAccount>(GITHUB_ACCOUNT), github_account());
    assert_eq!(read::<GithubApp>(GITHUB_APP), github_app());
    assert_eq!(read::<ModelList>(MODEL_LIST), model_list());
    assert_eq!(read::<HubUpdate>(HUB_UPDATE), hub_update());
    assert_eq!(read::<Records>(RECORDS), records());
}
