//! What a hub asks its worker of what a worktree changes, and the summary the worker sends back, spelled out, since a
//! worker can outlive the hub that started it, and the next hub can be of another build.

use protocol::frame;
use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, FileVersion, LineCounts};
use protocol::worker::{self, WorkerFrame, WorkerRequest, WorkerResponse};

use super::keeps_its_form;
use super::worker_calls::{answered, asked};

#[test]
fn the_request_for_a_summary_of_changes_keeps_its_form() {
    let asking = worker::SummarizeChanges { path: "/w".into(), base_ref: Some("origin/main".into()) };

    asked(
        WorkerRequest::SummarizeChanges(asking),
        r#"{"method":"summarize_changes","path":"/w","base_ref":"origin/main"}"#,
    );
}

#[test]
fn the_request_to_watch_a_worktree_s_changes_keeps_its_form() {
    let asking = worker::WatchChanges { path: "/w".into(), base_ref: Some("origin/main".into()), watching: true };

    asked(
        WorkerRequest::WatchChanges(asking),
        r#"{"method":"watch_changes","path":"/w","base_ref":"origin/main","watching":true}"#,
    );
}

#[test]
fn a_watched_worktree_s_summary_keeps_its_form() {
    let summary = ChangeSummary::new("c1".into(), Vec::new());

    keeps_its_form(
        &WorkerFrame::Changes { path: "/w".into(), summary },
        r#"{"frame":"changes","path":"/w","summary":{"base_commit":"c1","lines":{"added":0,"removed":0},"files":[]}}"#,
    );
}

#[test]
fn a_summary_of_changes_keeps_its_form() {
    let renamed = ChangedFile {
        path: "b.rs".into(),
        from: Some("a.rs".into()),
        status: ChangeStatus::Renamed,
        lines: Some(LineCounts { added: 2, removed: 1 }),
        version: Some(FileVersion::from("0123abcd".to_string())),
    };
    let deleted =
        ChangedFile { path: "c.png".into(), from: None, status: ChangeStatus::Deleted, lines: None, version: None };
    let summary = ChangeSummary::new("c1".into(), vec![renamed, deleted]);

    answered(
        WorkerResponse::ChangeSummary(summary),
        concat!(
            r#"{"result":"change_summary","base_commit":"c1","lines":{"added":2,"removed":1},"files":["#,
            r#"{"path":"b.rs","from":"a.rs","status":"renamed","lines":{"added":2,"removed":1},"version":"0123abcd"},"#,
            r#"{"path":"c.png","from":null,"status":"deleted","lines":null,"version":null}]}"#,
        ),
    );
}

/// A summary that leaves files out past the bound says how many, which a hub from before the bound skips, and one from a
/// worker from before the bound reads as leaving none out.
#[test]
fn a_summary_says_how_many_files_it_leaves_out_and_one_from_before_the_bound_leaves_none() {
    let summary = ChangeSummary { left_out: 12, ..ChangeSummary::new("c1".into(), Vec::new()) };
    let before = r#"{"result":"change_summary","base_commit":"c1","lines":{"added":0,"removed":0},"files":[]}"#;

    answered(
        WorkerResponse::ChangeSummary(summary),
        concat!(
            r#"{"result":"change_summary","base_commit":"c1","lines":{"added":0,"removed":0},"files":[],"#,
            r#""left_out":12}"#
        ),
    );
    let Ok(WorkerResponse::ChangeSummary(read)) = frame::decode::<WorkerResponse>(before.as_bytes()) else {
        panic!("{before}")
    };
    assert_eq!(read.left_out, 0);
}

#[test]
fn a_changed_file_from_a_newer_worker_reads_with_what_this_build_knows() {
    let line = br#"{"path":"d","status":"copied","mode":"executable"}"#;

    let file: ChangedFile = frame::decode(line).unwrap();

    assert_eq!(file.status, ChangeStatus::Unknown);
    assert_eq!((file.from, file.lines, file.version), (None, None, None), "fields it left out");
}
