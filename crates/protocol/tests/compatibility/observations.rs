//! Observations, spelled out, since a client and its hub can each be a different build: as the feed tells of one and
//! its removal, and in the list a client reads as it connects.

use protocol::client::HubNotice;
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, FileVersion, LineCounts, Observation, ObservationKey, Observed,
    RecordKey, Records, Rev,
};
use serde::Deserialize;

use crate::keeps_its_form;

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

/// A workspace's changed files, which a hub sends at the workspace's level, as a notice and as their removal.
#[test]
fn a_workspace_s_changed_files_keep_their_form_as_a_notice_and_a_removal() {
    let file = ChangedFile {
        path: "a.rs".into(),
        from: None,
        status: ChangeStatus::Modified,
        lines: Some(LineCounts { added: 3, removed: 1 }),
        version: Some(FileVersion::from("0123abcd".to_string())),
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
        serde_json::from_str(r#"{"notice":"observation","kind":"session_context","session_id":"s","rev":5}"#).unwrap();
    let key: HubNotice = serde_json::from_str(
        r#"{"notice":"observation_removed","observation":{"kind":"session_context","session_id":"s"},"rev":6}"#,
    )
    .unwrap();
    let freshness: Observed<LineCounts> = serde_json::from_str(r#"{"freshness":"stale","value":{"added":1}}"#).unwrap();

    assert_eq!(kind, HubNotice::Observation(Observation::Unknown));
    assert_eq!(key, HubNotice::ObservationRemoved { observation: ObservationKey::Unknown, rev: Rev::new(6) });
    assert_eq!(freshness, Observed::Unknown);
}
