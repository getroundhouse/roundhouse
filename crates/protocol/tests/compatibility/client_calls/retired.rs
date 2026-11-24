//! The requests a hub of this build no longer answers, spelled out as clients of earlier builds sent them: those only a
//! client of version 3 of the client protocol sent, which followed a session's log, the hub's records, a terminal and
//! what it showed of GitHub, each by a request of its own, and the read of a page of a workspace's changed files.

use protocol::client::ClientRequest;
use protocol::frame;

/// Each reads as a request this build doesn't know, which a hub refuses as `Unsupported` and carries on.
#[test]
fn every_request_a_hub_no_longer_answers_reads_as_one_it_doesnt_know() {
    let retired = [
        r#"{"method":"feed_subscribe","subscription_id":2}"#,
        r#"{"method":"session_subscribe","id":"s","from_seq":5,"subscription_id":3}"#,
        r#"{"method":"session_subscribe_places","id":"s","subagent":null,"from_place":1,"subscription_id":4}"#,
        r#"{"method":"session_unsubscribe","subscription_id":3}"#,
        r#"{"method":"github_subscribe","subscription_id":3,"shown":{"workspaces":["w"],"closely":["w"],"references":[]}}"#,
        r#"{"method":"terminal_watch","id":"t","subscription_id":4}"#,
        concat!(
            r#"{"method":"workspace_changed_files","id":"w","#,
            r#""after":{"untracked":true,"path":"node_modules/a.js"}}"#
        ),
    ];

    for line in retired {
        let request = frame::decode::<ClientRequest>(line.as_bytes()).unwrap();

        assert_eq!(request, ClientRequest::Unknown, "{line}");
    }
}
