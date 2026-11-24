//! A client's request for one page of one changed file's diff, and the hub's reply, as a client writes the one and
//! reads the other.

use protocol::client::{ClientRequest, ClientResponse, workspaces};
use protocol::frame;
use protocol::model::{FileDiff, FileDiffBody, FileVersion};

use super::{answered, asked};

/// A request names the tracked file at its path, and leaves `untracked` out, unless it says it's the untracked one.
#[test]
fn the_request_for_one_file_s_diff_keeps_its_form_and_reads_from_its_first_page_without_a_rename() {
    let renamed = workspaces::WorkspaceFileDiff {
        id: "w".into(),
        path: "b.rs".into(),
        untracked: false,
        from: Some("a.rs".into()),
        first_line: 120,
    };
    let first = workspaces::WorkspaceFileDiff { from: None, first_line: 0, ..renamed.clone() };
    let untracked = workspaces::WorkspaceFileDiff { untracked: true, ..first.clone() };

    asked(
        ClientRequest::WorkspaceFileDiff(renamed),
        r#"{"method":"workspace_file_diff","id":"w","path":"b.rs","from":"a.rs","first_line":120}"#,
    );
    asked(
        ClientRequest::WorkspaceFileDiff(first.clone()),
        r#"{"method":"workspace_file_diff","id":"w","path":"b.rs","first_line":0}"#,
    );
    asked(
        ClientRequest::WorkspaceFileDiff(untracked),
        r#"{"method":"workspace_file_diff","id":"w","path":"b.rs","untracked":true,"first_line":0}"#,
    );
    let shortest = br#"{"method":"workspace_file_diff","id":"w","path":"b.rs"}"#;
    assert_eq!(frame::decode::<ClientRequest>(shortest).unwrap(), ClientRequest::WorkspaceFileDiff(first));
}

#[test]
fn every_kind_of_file_diff_keeps_its_form() {
    let diff = |version: Option<&str>, body| FileDiff {
        base_commit: "c1".into(),
        version: version.map(|version| FileVersion::from(version.to_string())),
        body,
    };
    let patch = FileDiffBody::Patch { patch: "@@ -1 +1 @@\n-a\n+b\n".into(), next_line: Some(3) };
    let cases = [
        (
            diff(Some("0123abcd"), patch),
            r#"{"diff":"patch","patch":"@@ -1 +1 @@\n-a\n+b\n","next_line":3}"#,
            r#""version":"0123abcd""#,
        ),
        (
            diff(None, FileDiffBody::Patch { patch: String::new(), next_line: None }),
            r#"{"diff":"patch","patch":"","next_line":null}"#,
            r#""version":null"#,
        ),
        (diff(Some("0123abcd"), FileDiffBody::Binary), r#"{"diff":"binary"}"#, r#""version":"0123abcd""#),
        (diff(None, FileDiffBody::Unchanged), r#"{"diff":"unchanged"}"#, r#""version":null"#),
    ];

    for (diff, body, version) in cases {
        answered(
            ClientResponse::FileDiff(diff),
            &format!(r#"{{"result":"file_diff","base_commit":"c1",{version},"body":{body}}}"#),
        );
    }
}

#[test]
fn a_kind_of_file_diff_from_a_newer_hub_reads_as_unknown() {
    let line = br#"{"base_commit":"c1","version":null,"body":{"diff":"submodule","commits":2}}"#;

    let diff: FileDiff = frame::decode(line).unwrap();

    assert_eq!(diff.body, FileDiffBody::Unknown);
}
