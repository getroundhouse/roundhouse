use protocol::client::HubNotice;
use protocol::client::listed::SessionRecords;
use protocol::fixtures::{github_account, github_app, github_sign_in, github_user, tool_server};
use protocol::model::{
    AgentKind, DiffComment, DiffSide, GithubAccount, GithubAppId, GithubAppSaid, GithubRepositoryName, GithubSettings,
    GithubUser, GithubUserId, HubSettings, ModelList, ModelListKey, ModelLookup, Observation, Observed, Proposal,
    Reach, Record, RecordKey, Records, Repo, Rev, Session, Subagent, Terminal, Title, ToolServer, ToolServerName,
    ToolServerUrl, Workspace,
};

use crate::records::HubRecords;

fn repo(id: &str, name: &str) -> Repo {
    Repo { name: name.into(), rev: Rev::new(1), ..protocol::fixtures::repo(id) }
}

fn workspace(id: &str, created_at_ms: i64, rev: u64) -> Workspace {
    Workspace { created_at_ms, rev: Rev::new(rev), ..protocol::fixtures::workspace(id) }
}

fn session(id: &str, workspace: &str, created_at_ms: i64, rev: u64) -> Session {
    Session {
        workspace_id: workspace.into(),
        created_at_ms,
        title: Title::tidied(id),
        rev: Rev::new(rev),
        ..protocol::fixtures::session(id)
    }
}

fn comment(id: &str, workspace: &str, rev: u64) -> DiffComment {
    DiffComment {
        id: id.into(),
        workspace_id: workspace.into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "why".into(),
        created_at_ms: 0,
        rev: Rev::new(rev),
    }
}

fn terminal(id: &str, workspace: &str, opened_at_ms: i64, rev: u64) -> Terminal {
    Terminal { id: id.into(), workspace_id: workspace.into(), opened_at_ms, rev: Rev::new(rev) }
}

fn subagent(session: &str, call: &str, started_at_ms: i64, rev: u64) -> Subagent {
    Subagent { session_id: session.into(), started_at_ms, rev: Rev::new(rev), ..protocol::fixtures::subagent(call) }
}

/// Two workspaces, each with a session, and a comment on the first's diff and two terminals open in it.
fn listed() -> Records {
    Records {
        repos: vec![repo("r2", "zeta"), repo("r", "alpha")],
        workspaces: vec![workspace("w2", 20, 3), workspace("w", 10, 2)],
        sessions: vec![session("s2", "w", 30, 5), session("s", "w", 10, 4), session("t", "w2", 0, 6)],
        comments: vec![comment("c", "w", 7)],
        terminals: vec![terminal("t1", "w", 30, 3), terminal("t2", "w", 10, 1)],
        rev: Rev::new(7),
        ..Records::default()
    }
}

fn ids<T>(items: &[T], id: impl Fn(&T) -> &str) -> Vec<&str> {
    items.iter().map(id).collect()
}

#[test]
fn each_kind_is_listed_in_the_order_views_show_it_and_found_by_its_id() {
    let mut held = HubRecords::default();
    held.apply_list(listed());

    let shown = held.snapshot();

    assert_eq!(ids(shown.repos(), |r| r.name.as_str()), ["alpha", "zeta"], "by name");
    assert_eq!(ids(shown.workspaces(), |w| w.id.as_str()), ["w", "w2"], "oldest first");
    assert_eq!(ids(shown.sessions(&"w".into()), |s| s.id.as_str()), ["s", "s2"], "oldest first");
    assert_eq!(shown.session(&"t".into()).map(|s| s.workspace_id.as_str()), Some("w2"));
    assert_eq!(shown.workspace(&"w2".into()).map(|w| w.created_at_ms), Some(20));
    assert_eq!(shown.repo(&"r".into()).map(|r| r.name.as_str()), Some("alpha"));
    assert_eq!(ids(shown.comments(&"w".into()), |c| c.id.as_str()), ["c"]);
    assert!(shown.comments(&"w2".into()).is_empty() && shown.sessions(&"gone".into()).is_empty());
    assert_eq!(ids(shown.terminals(&"w".into()), |t| t.id.as_str()), ["t2", "t1"], "in the order they opened");
    assert!(shown.terminals(&"w2".into()).is_empty());
    assert_eq!(shown.every_session().count(), 3);
}

#[test]
fn a_snapshot_stays_as_it_was_taken_and_is_made_once_for_each_change() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    let before = held.snapshot();

    assert!(!held.snapshot().changes_since(&before).any(), "nothing changed, so it's the same one");
    held.apply_notice(HubNotice::Session(session("s", "w", 10, 8)));

    assert_eq!(before.session(&"s".into()).map(|s| s.rev), Some(Rev::new(4)));
    assert_eq!(held.snapshot().session(&"s".into()).map(|s| s.rev), Some(Rev::new(8)));
}

#[test]
fn a_view_is_told_which_parts_changed_since_it_last_looked() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    let before = held.snapshot();

    held.apply_notice(HubNotice::Session(session("s2", "w", 30, 8)));
    held.apply_notice(HubNotice::Removed { record: RecordKey::Comment { id: "c".into() }, rev: Rev::new(9) });
    held.apply_notice(HubNotice::Terminal(terminal("t3", "w2", 40, 10)));
    let now = held.snapshot();
    let changes = now.changes_since(&before);

    assert!(changes.any());
    assert!(changes.sessions(&"w".into()) && !changes.sessions(&"w2".into()), "only the workspace it's in");
    assert!(changes.comments(&"w".into()), "one went");
    assert!(changes.terminals(&"w2".into()) && !changes.terminals(&"w".into()), "one opened, in the second");
    assert!(!changes.workspace(&"w".into()) && !changes.workspace(&"w2".into()), "no workspace's own record");
    assert!(!changes.model_lists(), "no list of models");
    assert!(!now.changes_since(&now).any());
    let key = ModelListKey {
        agent: AgentKind::Codex,
        repo: "r".into(),
        provider: "local".into(),
        account: None,
        cli_version: None,
    };
    held.apply_record(Record::ModelList(ModelList { key, lookup: ModelLookup::Asking, rev: Rev::new(11) }));
    assert!(held.snapshot().changes_since(&now).model_lists(), "one came");
}

#[test]
fn a_terminal_that_closes_goes_from_its_workspace() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    let before = held.snapshot();

    held.apply_notice(HubNotice::Removed { record: RecordKey::Terminal { id: "t1".into() }, rev: Rev::new(8) });
    let now = held.snapshot();

    assert_eq!(ids(now.terminals(&"w".into()), |t| t.id.as_str()), ["t2"]);
    assert!(now.changes_since(&before).terminals(&"w".into()));
}

#[test]
fn a_record_that_goes_is_a_change_even_when_it_was_not_the_newest() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    held.apply_notice(HubNotice::Session(session("s", "w", 10, 8)));
    let before = held.snapshot();

    held.apply_notice(HubNotice::Removed { record: RecordKey::Session { id: "s2".into() }, rev: Rev::new(9) });
    let now = held.snapshot();

    assert!(now.changes_since(&before).sessions(&"w".into()));
    assert_eq!(ids(now.sessions(&"w".into()), |s| s.id.as_str()), ["s"]);
}

#[test]
fn the_same_copy_listed_again_is_no_change_to_a_view() {
    let mut held = HubRecords::default();
    held.apply_list(listed());
    let before = held.snapshot();

    let mut again = listed();
    again.rev = Rev::new(8);
    held.apply_list(again);
    let changes_now = held.snapshot();
    let changes = changes_now.changes_since(&before);

    assert!(!changes.sessions(&"w".into()) && !changes.workspace(&"w".into()) && !changes.comments(&"w".into()));
}

/// A session's subagents are listed in the order they started, each found by its call, and go as the feed removes
/// them, as any record does.
#[test]
fn a_sessions_subagents_are_listed_as_they_started_and_found_by_their_call() {
    let mut held = HubRecords::default();
    held.want_session(&"s".into());
    let subagents = vec![subagent("s", "b", 2, 5), subagent("s", "a", 1, 4)];
    let listed = SessionRecords {
        session_id: "s".into(),
        subagents,
        proposals: Vec::new(),
        replies_heard: Vec::new(),
        observations: Vec::new(),
        rev: Rev::new(5),
    };
    held.apply_session_list(listed);
    let before = held.snapshot();

    held.apply_notice(HubNotice::Subagent(subagent("s", "a", 1, 6)));
    let now = held.snapshot();

    assert_eq!(ids(now.subagents(&"s".into()), |s| s.call.as_str()), ["a", "b"]);
    assert_eq!(now.subagent(&"s".into(), &"a".into()).map(|s| s.rev), Some(Rev::new(6)));
    assert!(now.changes_since(&before).subagents(&"s".into()));
    assert!(!now.changes_since(&before).subagents(&"t".into()));
    let unchanged = |snapshot: &super::Snapshot| snapshot.subagent(&"s".into(), &"b".into()).cloned().unwrap();
    assert!(std::sync::Arc::ptr_eq(&unchanged(&before), &unchanged(&now)), "one that didn't change is shared");
    let gone = RecordKey::Subagent { session_id: "s".into(), subagent: subagent("s", "b", 2, 5).id.clone() };
    assert!(held.apply_notice(HubNotice::Removed { record: gone, rev: Rev::new(7) }), "removed, as a record can be");
    assert_eq!(ids(held.snapshot().subagents(&"s".into()), |s| s.call.as_str()), ["a"]);
}

/// Two accounts of one user, one of another and one whose username isn't known, a sign-in, and three apps of your own
/// and one whose name isn't known, each named by the hub's observation of it, whatever its record says.
#[test]
fn what_a_hub_says_of_github_is_listed_in_the_order_views_show_it_and_found_by_its_id() {
    let account = |id: &str, number: u64| GithubAccount { user: github_user(number), ..github_account(id) };
    let named = |id: &str, name: &str| Observation::GithubUsername {
        account: id.into(),
        username: Observed::Current { value: name.into() },
        rev: Rev::new(1),
    };
    let app_said = |id: u64, name: &str| Observation::GithubAppSaid {
        app: GithubAppId::new(id),
        said: Observed::Current {
            value: GithubAppSaid {
                slug: name.into(),
                name: name.to_string(),
                owner: GithubUser { id: GithubUserId::new(1), name: "ada".into() },
            },
        },
        rev: Rev::new(1),
    };
    let mut held = HubRecords::default();

    held.apply_list(Records {
        github_accounts: vec![account("g4", 7), account("g3", 2), account("g2", 1), account("g1", 1)],
        github_sign_ins: vec![github_sign_in("s")],
        github_apps: [6, 9, 8, 7].map(github_app).to_vec(),
        observations: vec![
            named("g3", "bea"),
            named("g2", "Ada"),
            named("g1", "ada"),
            app_said(9, "bea-roundhouse"),
            app_said(8, "Ada-roundhouse"),
            app_said(7, "ada-roundhouse"),
        ],
        rev: Rev::new(1),
        ..Records::default()
    });
    let shown = held.snapshot();

    let accounts: Vec<&str> = shown.github_accounts().iter().map(|a| a.id.as_str()).collect();
    assert_eq!(accounts, ["g1", "g2", "g3", "g4"], "by name in any case, then ID, then the one not named");
    let usernames: Vec<String> = shown.github_accounts().iter().map(|a| shown.github_username(a).to_string()).collect();
    assert_eq!(usernames, ["ada", "Ada", "bea", "#7"], "by its number, where its name isn't known");
    assert_eq!(shown.github_sign_in(&"s".into()).map(|s| s.status.is_waiting()), Some(true));
    assert_eq!(shown.github_sign_ins().count(), 1);
    let apps: Vec<u64> = shown.github_apps().iter().map(|a| a.id.get()).collect();
    assert_eq!(apps, [7, 8, 9, 6], "by name in any case, then number, then the one not named");
    let nine = shown.github_app(GithubAppId::new(9)).unwrap();
    assert_eq!(shown.github_app_said(nine).map(|said| said.name.as_str()), Some("bea-roundhouse"));
    assert!(shown.github_account(&"gone".into()).is_none() && shown.github_sign_in(&"gone".into()).is_none());
    assert!(shown.github_app(GithubAppId::new(1)).is_none());
}

/// What a push from `r`'s workspaces would meet, as the hub's level sends it.
fn reach_of(repo: &str, reach: Reach, rev: u64) -> Observation {
    Observation::RepositoryReach { repo: repo.into(), reach: Observed::Current { value: reach }, rev: Rev::new(rev) }
}

#[test]
fn a_workspace_says_what_a_push_from_it_meets_by_its_repositorys_reach() {
    let repository = GithubRepositoryName::named("example/project");
    let reason = "Permission denied to ada.".into();
    let refused = Reach::Refused { repository, username: "ada".into(), reason, installation: None };
    let mut held = HubRecords::default();
    held.apply_list(Records { observations: vec![reach_of("r", refused.clone(), 6)], ..listed() });

    let shown = held.snapshot();
    assert_eq!(shown.github_reach(&"r".into()), Some(&refused));
    assert_eq!(shown.in_workspace(&"w".into()).unwrap().github_reach(), Some(&refused), "its repository's");
    assert_eq!(shown.github_reach(&"r2".into()), None, "not checked yet");

    held.apply_notice(HubNotice::Observation(reach_of("r", Reach::NotOnGithub, 8)));
    assert_eq!(held.snapshot().in_workspace(&"w".into()).unwrap().github_reach(), Some(&Reach::NotOnGithub));
}

/// A workspace on a provider whose machines the hub's settings say Roundhouse leaves GitHub to says nothing of its
/// repository's reach, since its pushes go as the machine sends them. One on any other provider says it.
#[test]
fn a_workspace_on_a_provider_left_to_its_machine_says_nothing_of_its_repositorys_reach() {
    let repository = GithubRepositoryName::named("example/project");
    let refused = Reach::Refused { repository, username: "ada".into(), reason: "No.".into(), installation: None };
    let observations = vec![reach_of("r", refused.clone(), 2)];
    let on_vms = Workspace { provider: "vms".into(), ..workspace("w2", 20, 3) };
    let github =
        GithubSettings { account: None, sent: None, left_to_machine: vec!["local".into()], set_up: Vec::new() };
    let hub_settings = HubSettings {
        keep_awake: false,
        allow_asked_in_full_access: None,
        account_defaults: None,
        github: Some(github),
        rev: Rev::new(1),
    };
    let mut held = HubRecords::default();
    held.apply_list(Records {
        workspaces: vec![on_vms, workspace("w", 10, 2)],
        observations,
        hub_settings: Some(hub_settings),
        ..listed()
    });

    let shown = held.snapshot();
    assert_eq!(shown.in_workspace(&"w".into()).unwrap().github_reach(), None, "on the local provider");
    assert_eq!(shown.in_workspace(&"w2".into()).unwrap().github_reach(), Some(&refused));
}

/// A session's proposals come and go with its subagents, as a view follows and lets go of the session, and a change to
/// one is told of apart from them.
#[test]
fn a_sessions_proposals_are_followed_with_its_subagents_and_their_changes_told_apart() {
    let mut held = HubRecords::default();
    let proposal = |rev: u64| Proposal { rev: Rev::new(rev), ..protocol::fixtures::proposal("p") };
    assert!(!held.apply_notice(HubNotice::Proposal(proposal(1))), "nobody follows the session");
    held.want_session(&"s".into());
    let before = held.snapshot();

    held.apply_notice(HubNotice::Proposal(proposal(2)));
    let now = held.snapshot();

    let changes = now.changes_since(&before);
    assert!(changes.any() && changes.proposals(&"s".into()) && !changes.subagents(&"s".into()));
    assert_eq!(now.proposal(&"s".into(), &"p".into()).map(|p| p.rev), Some(Rev::new(2)));
    assert!(held.forget_session(&"s".into()));
    assert!(held.snapshot().proposals(&"s".into()).is_empty());
    assert!(!held.apply_notice(HubNotice::Proposal(proposal(3))), "let go of, so nothing of it is kept");
}

#[test]
fn a_url_someone_types_finds_the_first_tool_server_by_name_thats_at_it() {
    let at = |id: &str, name: &str, url: &str| ToolServer {
        name: ToolServerName::stored(name.to_owned()),
        url: ToolServerUrl::stored(url.to_owned()),
        ..tool_server(id)
    };
    let url = "https://mcp.example.com/mcp";
    let mut held = HubRecords::default();
    let second = at("a", "Linear (Northwind)", url);
    let other = at("c", "Docs", "https://docs.example.com/mcp");
    held.apply_list(Records { tool_servers: vec![second, at("b", "Linear", url), other], ..Records::default() });
    let shown = held.snapshot();

    let found = |typed: &str| shown.tool_server_at(typed).map(|server| server.id.as_str());

    assert_eq!(found(&format!(" {url}\n")), Some("b"));
    assert_eq!(found("https://docs.example.com/mcp"), Some("c"));
    assert_eq!(found("https://mcp.example.com/"), None);
    assert_eq!(found("mcp"), None);
}
