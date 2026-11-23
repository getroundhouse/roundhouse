//! Observations, spelled out, since a client and its hub can each be a different build: as the feed tells of one and
//! its removal, and in the list a client reads as it connects.

use protocol::capability::output::TodoList;
use protocol::client::listed::{Listed, ReferenceObservations, SessionRecords};
use protocol::client::{HubFrame, HubNotice};
use protocol::model::{
    About, AccountIdentity, ChangeStatus, ChangeSummary, ChangedFile, ContextUsage, FileVersion, GithubRepositoryName,
    IssueNumber, IssueSaid, IssueState, IssueSummary, LimitWindow, LineCounts, Observation, ObservationKey, Observed,
    RecordKey, Records, ReferenceAsk, ReferenceSaid, Rev, UsageLimit,
};
use serde::Deserialize;

use crate::keeps_its_form;
use crate::workspace_pull_requests::pull_request_said;

fn lines(lines: Observed<LineCounts>) -> Observation {
    Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(5) }
}

const COUNTS: &str = r#"{"added":3,"removed":1}"#;

#[test]
fn an_observation_keeps_its_form_however_fresh_as_a_notice_and_a_removal() {
    let counts = LineCounts { added: 3, removed: 1 };
    let notice = |freshness: &str| {
        format!(r#"{{"notice":"observation","kind":"workspace_lines","workspace_id":"w","lines":{freshness},"rev":5}}"#)
    };

    keeps_its_form(&HubNotice::Observation(lines(Observed::NotKnown)), &notice(r#"{"freshness":"not_known"}"#));
    let current = format!(r#"{{"freshness":"current","value":{COUNTS}}}"#);
    keeps_its_form(&HubNotice::Observation(lines(Observed::Current { value: counts })), &notice(&current));
    let as_of = format!(r#"{{"freshness":"as_of","value":{COUNTS},"as_of_ms":1795230000000}}"#);
    let read_then = Observed::AsOf { value: counts, as_of_ms: 1_795_230_000_000 };
    keeps_its_form(&HubNotice::Observation(lines(read_then)), &notice(&as_of));
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::WorkspaceLines { workspace_id: "w".into() },
        rev: Rev::new(6),
    };
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"workspace_lines","workspace_id":"w"},"rev":6}"#,
    );
}

/// Who an agent's account is, which a hub sends at its own level to a client of a version that reads it so, as a
/// notice and as its removal.
#[test]
fn who_an_account_is_keeps_its_form_as_a_notice_and_a_removal() {
    let identity = AccountIdentity {
        model_provider: Some("Anthropic".into()),
        plan: Some("max".into()),
        login: Some("Claude account".into()),
        email: Some("ada@example.com".into()),
    };
    let identity = Observed::AsOf { value: identity, as_of_ms: 1_795_230_000_000 };
    let copy =
        HubNotice::Observation(Observation::AccountIdentity { account: "claude-1".into(), identity, rev: Rev::new(5) });
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::AccountIdentity { account: "claude-1".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy,
        r#"{"notice":"observation","kind":"account_identity","account":"claude-1","identity":{"freshness":"as_of","value":{"model_provider":"Anthropic","plan":"max","login":"Claude account","email":"ada@example.com"},"as_of_ms":1795230000000},"rev":5}"#,
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"account_identity","account":"claude-1"},"rev":6}"#,
    );
}

/// What's left of an account's limits, which a hub sends at its own level to a client of a version that reads it so,
/// each limit with when it was heard, as a notice and as its removal.
#[test]
fn an_account_s_limits_keep_their_form_as_a_notice_and_a_removal() {
    let week = UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 40,
        resets_at_ms: Some(1_795_400_000_000),
        heard_at_ms: Some(1_795_230_000_000),
    };
    let limits = Observed::AsOf { value: vec![week], as_of_ms: 1_795_230_000_000 };
    let copy =
        HubNotice::Observation(Observation::AccountLimits { account: "claude-1".into(), limits, rev: Rev::new(5) });
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::AccountLimits { account: "claude-1".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy,
        r#"{"notice":"observation","kind":"account_limits","account":"claude-1","limits":{"freshness":"as_of","value":[{"window":"seven_days","used_percent":40,"resets_at_ms":1795400000000,"heard_at_ms":1795230000000}],"as_of_ms":1795230000000},"rev":5}"#,
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"account_limits","account":"claude-1"},"rev":6}"#,
    );
}

/// How full a session's context is, and its to-do list, which a hub sends at the session's level to a client of a
/// version that reads them so: as a notice, as a removal, and in the session's list.
#[test]
fn a_session_s_context_and_todos_keep_their_form_as_notices_and_in_its_list() {
    let context = ContextUsage {
        used_tokens: 60_000,
        max_tokens: 200_000,
        model_tokens: None,
        max_set_by: None,
        parts: Vec::new(),
    };
    let context = Observed::AsOf { value: context, as_of_ms: 1_795_230_000_000 };
    let copy = Observation::SessionContext { session_id: "s".into(), context, rev: Rev::new(5) };
    let todos = TodoList { items: Vec::new(), note: Some("Nothing left".into()) };
    let todos = Observation::SessionTodos {
        session_id: "s".into(),
        todos: Observed::AsOf { value: todos, as_of_ms: 1_795_230_000_000 },
        rev: Rev::new(6),
    };
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::SessionContext { session_id: "s".into() },
        rev: Rev::new(7),
    };
    let listed = Listed::Session(SessionRecords {
        session_id: "s".into(),
        subagents: Vec::new(),
        proposals: Vec::new(),
        replies_heard: Vec::new(),
        observations: vec![todos.clone()],
        rev: Rev::new(8),
    });

    keeps_its_form(
        &HubNotice::Observation(copy),
        concat!(
            r#"{"notice":"observation","kind":"session_context","session_id":"s","context":{"freshness":"as_of","#,
            r#""value":{"used_tokens":60000,"max_tokens":200000,"parts":[]},"as_of_ms":1795230000000},"rev":5}"#
        ),
    );
    keeps_its_form(
        &HubNotice::Observation(todos),
        concat!(
            r#"{"notice":"observation","kind":"session_todos","session_id":"s","todos":{"freshness":"as_of","#,
            r#""value":{"items":[],"note":"Nothing left"},"as_of_ms":1795230000000},"rev":6}"#
        ),
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"session_context","session_id":"s"},"rev":7}"#,
    );
    keeps_its_form(
        &HubFrame::Listed { subscription_id: 2, listed },
        concat!(
            r#"{"frame":"listed","subscription_id":2,"listed":{"level":"session","session_id":"s","subagents":[],"#,
            r#""proposals":[],"replies_heard":[],"observations":[{"kind":"session_todos","session_id":"s","#,
            r#""todos":{"freshness":"as_of","value":{"items":[],"note":"Nothing left"},"as_of_ms":1795230000000},"#,
            r#""rev":6}],"rev":8}}"#
        ),
    );
}

/// A workspace's changed files, which a hub sends at the workspace's level, as a notice and as their removal.
#[test]
fn a_workspace_s_changed_files_keep_their_form_as_a_notice_and_a_removal() {
    let file = ChangedFile {
        path: "a.rs".into(),
        from: None,
        status: ChangeStatus::Modified,
        lines: Some(LineCounts { added: 3, removed: 1 }),
        version: Some(FileVersion::from("0123abcd".to_string())),
        name_not_utf8: false,
    };
    let summary = ChangeSummary { left_out: 2, ..ChangeSummary::new("c1".into(), vec![file]) };
    let changes = Observed::Current { value: summary };
    let copy =
        HubNotice::Observation(Observation::WorkspaceChanges { workspace_id: "w".into(), changes, rev: Rev::new(5) });
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::WorkspaceChanges { workspace_id: "w".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy,
        concat!(
            r#"{"notice":"observation","kind":"workspace_changes","workspace_id":"w","changes":{"freshness":"current","#,
            r#""value":{"base_commit":"c1","lines":{"added":3,"removed":1},"files":[{"path":"a.rs","from":null,"#,
            r#""status":"modified","lines":{"added":3,"removed":1},"version":"0123abcd"}],"left_out":2}},"rev":5}"#
        ),
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"workspace_changes","workspace_id":"w"},"rev":6}"#,
    );
}

#[test]
fn a_list_keeps_its_observations_under_its_own_revision() {
    let listed =
        Records { observations: vec![lines(Observed::Current { value: LineCounts::default() })], ..Records::default() };

    let json = serde_json::to_string(&listed).unwrap();

    let observation = r#"{"kind":"workspace_lines","workspace_id":"w","lines":{"freshness":"current","value":{"added":0,"removed":0}},"rev":5}"#;
    assert!(json.contains(&format!(r#""observations":[{observation}],"rev":0"#)), "{json}");
}

/// The feed's notices as a client from before observations reads them: every kind of record, a removal of one, and
/// `Resync`, with anything else unknown.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "notice", rename_all = "snake_case")]
enum NoticeBefore {
    Removed {
        record: RecordKey,
        rev: Rev,
    },
    Resync,
    #[serde(other)]
    Unknown,
}

/// A list as a client from before observations reads it.
#[derive(Deserialize, Debug, PartialEq)]
struct RecordsBefore {
    repos: Vec<serde_json::Value>,
    rev: Rev,
}

/// A client from before skips both notices and the list's new field, and this one reads a list from a hub from before
/// as having none.
#[test]
fn a_build_from_before_skips_what_this_one_sends_and_this_one_reads_what_that_sent() {
    let copy = serde_json::to_string(&HubNotice::Observation(lines(Observed::NotKnown))).unwrap();
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::WorkspaceLines { workspace_id: "w".into() },
        rev: Rev::new(6),
    };
    let gone = serde_json::to_string(&gone).unwrap();
    let listed = Records { observations: vec![lines(Observed::NotKnown)], rev: Rev::new(7), ..Records::default() };
    let before: Records =
        serde_json::from_str(r#"{"repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":4}"#).unwrap();

    assert_eq!(serde_json::from_str::<NoticeBefore>(&copy).unwrap(), NoticeBefore::Unknown, "{copy}");
    assert_eq!(serde_json::from_str::<NoticeBefore>(&gone).unwrap(), NoticeBefore::Unknown, "{gone}");
    let read = serde_json::from_str::<RecordsBefore>(&serde_json::to_string(&listed).unwrap()).unwrap();
    assert_eq!(read, RecordsBefore { repos: Vec::new(), rev: Rev::new(7) });
    assert_eq!(before.observations, Vec::new());
    assert!(!serde_json::to_string(&before).unwrap().contains("observations"));
}

/// What a newer hub may send: a kind of observation, a kind of key, or a freshness this build doesn't know, each read as
/// unknown rather than failing the notice.
#[test]
fn a_newer_kind_or_freshness_reads_as_unknown() {
    let kind: HubNotice =
        serde_json::from_str(r#"{"notice":"observation","kind":"weather","station":"s","rev":5}"#).unwrap();
    let key: HubNotice = serde_json::from_str(
        r#"{"notice":"observation_removed","observation":{"kind":"weather","station":"s"},"rev":6}"#,
    )
    .unwrap();
    let freshness: Observed<LineCounts> = serde_json::from_str(r#"{"freshness":"stale","value":{"added":1}}"#).unwrap();

    assert_eq!(kind, HubNotice::Observation(Observation::Unknown));
    assert_eq!(key, HubNotice::ObservationRemoved { observation: ObservationKey::Unknown, rev: Rev::new(6) });
    assert_eq!(freshness, Observed::Unknown);
}

/// The issue a chip names in these, as its observation says it, and the ask it's of, as the wire spells it.
fn chip() -> (ReferenceAsk, ReferenceSaid, &'static str) {
    let reference =
        ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(41).unwrap() };
    let about = About {
        url: "https://github.com/o/r/issues/41".into(),
        title: "Decide".into(),
        author: None,
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    };
    let issue = ReferenceSaid::Issue(IssueSaid {
        repository: GithubRepositoryName::named("o/r"),
        number: IssueNumber::try_from(41).unwrap(),
        summary: IssueSummary { about, state: IssueState::Open },
    });
    (reference, issue, r#""reference":{"repository":"o/r","number":41}"#)
}

/// What GitHub says of a chip's issue or pull request keeps its form as an observation, found, missing or gone, and
/// a kind of it from a newer hub reads as unknown.
#[test]
fn a_chip_s_observation_keeps_its_form() {
    let (reference, issue, ask) = chip();
    let of = |said| Observation::Reference { reference: reference.clone(), said, rev: Rev::new(7) };

    keeps_its_form(
        &HubNotice::Observation(of(Observed::Current { value: issue })),
        &[
            r#"{"notice":"observation","kind":"reference","#,
            ask,
            r#","said":{"freshness":"current","value":{"type":"issue","repository":"o/r","number":41,"summary":"#,
            r#"{"about":{"url":"https://github.com/o/r/issues/41","title":"Decide","opened_at_ms":1,"comments":0,"#,
            r#""labels":[],"body":""},"state":"open"}}},"rev":7}"#,
        ]
        .concat(),
    );
    let (pull_request, summary) = pull_request_said();
    keeps_its_form(
        &HubNotice::Observation(of(Observed::Current { value: ReferenceSaid::PullRequest(pull_request) })),
        &[
            r#"{"notice":"observation","kind":"reference","#,
            ask,
            r#","said":{"freshness":"current","value":{"type":"pull_request","#,
            summary,
            r#"}},"rev":7}"#,
        ]
        .concat(),
    );
    keeps_its_form(
        &HubNotice::Observation(of(Observed::AsOf { value: ReferenceSaid::Missing, as_of_ms: 1_000 })),
        &[
            r#"{"notice":"observation","kind":"reference","#,
            ask,
            r#","said":{"freshness":"as_of","value":{"type":"missing"},"as_of_ms":1000},"rev":7}"#,
        ]
        .concat(),
    );
    keeps_its_form(
        &HubNotice::ObservationRemoved { observation: ObservationKey::Reference { reference }, rev: Rev::new(8) },
        &[r#"{"notice":"observation_removed","observation":{"kind":"reference","#, ask, r#"},"rev":8}"#].concat(),
    );
    let newer = r#"{"freshness":"current","value":{"type":"discussion","number":3}}"#;
    let read: Observed<ReferenceSaid> = serde_json::from_str(newer).unwrap();
    assert_eq!(read, Observed::Current { value: ReferenceSaid::Unknown }, "a kind from a newer hub");
}

/// The list a chip's level sends first keeps its form.
#[test]
fn a_chip_s_list_keeps_its_form() {
    let (reference, _, ask) = chip();
    let missing = Observed::AsOf { value: ReferenceSaid::Missing, as_of_ms: 1_000 };
    let observation = Observation::Reference { reference: reference.clone(), said: missing, rev: Rev::new(7) };
    let listed = ReferenceObservations { reference, observations: vec![observation], rev: Rev::new(9) };

    keeps_its_form(
        &HubFrame::Listed { subscription_id: 3, listed: Listed::Reference(listed) },
        &[
            r#"{"frame":"listed","subscription_id":3,"listed":{"level":"reference","#,
            ask,
            r#","observations":[{"kind":"reference","#,
            ask,
            r#","said":{"freshness":"as_of","value":{"type":"missing"},"as_of_ms":1000},"rev":7}],"rev":9}}"#,
        ]
        .concat(),
    );
}
