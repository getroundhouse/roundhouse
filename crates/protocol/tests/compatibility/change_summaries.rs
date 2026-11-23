//! What a hub asks its worker of what a worktree changes, and what the worker sends back, the summary or one file's
//! diff, spelled out, since a worker can outlive the hub that started it, and the next hub can be of another build.
//! And a client's read of a page of the files a summary leaves out, which the hub passes on to the worker.

use protocol::client::workspaces::WorkspaceChangedFiles;
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, ReplyOutcome};
use protocol::frame;
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, ChangedFileAt, ChangedFilesPage, FileDiff, FileDiffBody, FileVersion,
    LineCounts,
};
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

/// Where a page starts: after the file a summary or the page before ended with.
fn after() -> ChangedFileAt {
    ChangedFileAt { untracked: true, path: "node_modules/a.js".into() }
}

/// A page of one file, which is the last.
fn page() -> ChangedFilesPage {
    let file = ChangedFile {
        path: "node_modules/b.js".into(),
        from: None,
        status: ChangeStatus::Untracked,
        lines: None,
        version: None,
    };
    ChangedFilesPage { base_commit: "c1".into(), files: vec![file], left_out: 0 }
}

const AFTER: &str = r#""after":{"untracked":true,"path":"node_modules/a.js"}"#;
const PAGE: &str = concat!(
    r#""base_commit":"c1","files":[{"path":"node_modules/b.js","from":null,"status":"untracked","lines":null,"#,
    r#""version":null}],"left_out":0"#
);

#[test]
fn a_client_s_read_of_a_page_of_changed_files_and_its_reply_keep_their_form() {
    let asking = ClientRequest::WorkspaceChangedFiles(WorkspaceChangedFiles { id: "w".into(), after: after() });

    keeps_its_form(
        &ClientFrame { id: 7, request: asking },
        &format!(r#"{{"id":7,"request":{{"method":"workspace_changed_files","id":"w",{AFTER}}}}}"#),
    );
    keeps_its_form(
        &HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(ClientResponse::ChangedFilesPage(page())) },
        &format!(r#"{{"frame":"reply","id":7,"response":{{"result":"changed_files_page",{PAGE}}}}}"#),
    );
}

#[test]
fn the_request_for_a_page_of_changed_files_and_its_reply_keep_their_form() {
    let asking = worker::ListChangedFiles { path: "/w".into(), base_ref: Some("origin/main".into()), after: after() };

    asked(
        WorkerRequest::ListChangedFiles(asking),
        &format!(r#"{{"method":"list_changed_files","path":"/w","base_ref":"origin/main",{AFTER}}}"#),
    );
    answered(WorkerResponse::ChangedFilesPage(page()), &format!(r#"{{"result":"changed_files_page",{PAGE}}}"#));
}

/// A request names the tracked file at its path, as a hub from before `untracked` asked for one, unless it says it's the
/// untracked one, so a worker of this build reads that hub's as it did.
#[test]
fn the_request_for_one_file_s_diff_and_its_reply_keep_their_forms() {
    let asking = worker::DiffFile {
        path: "/w".into(),
        base_ref: Some("origin/main".into()),
        file: "b.rs".into(),
        untracked: false,
        from: Some("a.rs".into()),
        first_line: 0,
    };
    let untracked = worker::DiffFile { file: "a.rs".into(), untracked: true, from: None, ..asking.clone() };
    let diff = FileDiff {
        base_commit: "c1".into(),
        version: Some(FileVersion::from("0123abcd".to_string())),
        body: FileDiffBody::Patch { patch: "+b\n".into(), next_line: None },
    };

    asked(
        WorkerRequest::DiffFile(asking),
        r#"{"method":"diff_file","path":"/w","base_ref":"origin/main","file":"b.rs","from":"a.rs","first_line":0}"#,
    );
    asked(
        WorkerRequest::DiffFile(untracked),
        concat!(
            r#"{"method":"diff_file","path":"/w","base_ref":"origin/main","file":"a.rs","untracked":true,"from":null,"#,
            r#""first_line":0}"#,
        ),
    );
    answered(
        WorkerResponse::FileDiff(diff),
        concat!(
            r#"{"result":"file_diff","base_commit":"c1","version":"0123abcd","#,
            r#""body":{"diff":"patch","patch":"+b\n","next_line":null}}"#,
        ),
    );
}
