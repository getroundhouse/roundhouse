//! Where each repository is, spelled out: its record, as the feed and a list carry it, and the request that reads a
//! checkout's address and its reply, since a worker can outlive the hub that started it.

use protocol::client::HubNotice;
use protocol::frame;
use protocol::model::{Repo, RepoOrigin, RepoUrl, Rev};
use protocol::worker::{ReadRepoUrl, RepoUrlReply, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};

use super::keeps_its_form;
use super::worker_calls::{answered, asked};

fn roundhouse() -> RepoUrl {
    RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap()
}

fn repo(origin: RepoOrigin) -> HubNotice {
    HubNotice::Repo(Repo { id: "r".into(), name: "repo".into(), origin, created_at_ms: 1, rev: Rev::new(2) })
}

#[test]
fn a_repository_keeps_its_form_wherever_it_is() {
    let url = r#""url":"https://github.com/ada/roundhouse.git""#;

    keeps_its_form(
        &repo(RepoOrigin::Checkout { path: "/repo".into() }),
        r#"{"notice":"repo","id":"r","name":"repo","origin":{"kind":"checkout","path":"/repo"},"created_at_ms":1,"rev":2}"#,
    );
    keeps_its_form(
        &repo(RepoOrigin::Linked { path: "/repo".into(), url: roundhouse() }),
        &format!(
            r#"{{"notice":"repo","id":"r","name":"repo","origin":{{"kind":"linked","path":"/repo",{url}}},"created_at_ms":1,"rev":2}}"#
        ),
    );
    keeps_its_form(
        &repo(RepoOrigin::Remote { url: roundhouse() }),
        &format!(
            r#"{{"notice":"repo","id":"r","name":"repo","origin":{{"kind":"remote",{url}}},"created_at_ms":1,"rev":2}}"#
        ),
    );
}

/// From a newer hub, which may have somewhere this build has no word for.
#[test]
fn a_repository_somewhere_this_build_does_not_know_still_reads() {
    let newer = br#"{"notice":"repo","id":"r","name":"repo","origin":{"kind":"mirrored","from":"x"},"created_at_ms":1,"rev":2}"#;

    let notice: HubNotice = frame::decode(newer).unwrap();

    assert_eq!(notice, repo(RepoOrigin::Unknown));
}

#[test]
fn reading_a_checkouts_address_keeps_its_form() {
    let reply = |url| WorkerResponse::RepoUrl(RepoUrlReply { url });

    asked(
        WorkerRequest::ReadRepoUrl(ReadRepoUrl { path: "/repo".into() }),
        r#"{"method":"read_repo_url","path":"/repo"}"#,
    );
    answered(reply(Some(roundhouse())), r#"{"result":"repo_url","url":"https://github.com/ada/roundhouse.git"}"#);
    answered(reply(None), r#"{"result":"repo_url","url":null}"#);
}

/// A worker of protocol 5 says where pushes from the checkout go, as `push`, or says none, as `null`, which the hub
/// skips, as it skips any field it doesn't know.
#[test]
fn a_reply_that_says_where_pushes_go_reads_as_its_address() {
    let line = |push: &str| {
        format!(
            r#"{{"frame":"reply","id":7,"response":{{"result":"repo_url","url":"https://github.com/ada/roundhouse.git","push":{push}}}}}"#
        )
    };
    let read = |line: String| frame::decode::<WorkerFrame>(line.as_bytes()).unwrap();

    for push in [r#""credential""#, r#""around""#, "null"] {
        let reply = WorkerResponse::RepoUrl(RepoUrlReply { url: Some(roundhouse()) });
        assert_eq!(read(line(push)), WorkerFrame::Reply { id: 7, outcome: WorkerOutcome::Response(reply) }, "{push}");
    }
}

/// A hub keeps no address with a credential, so one in a worker's reply fails that call, as any reply the hub can't
/// read does, without saying the credential.
#[test]
fn a_reply_whose_address_holds_a_credential_is_refused_by_its_call() {
    let line = br#"{"frame":"reply","id":7,"response":{"result":"repo_url","url":"https://ada:ghp_made_up@github.com/ada/roundhouse.git"}}"#;

    let read = frame::decode::<WorkerFrame>(line);

    let said = read.unwrap_err().to_string();
    assert!(!said.contains("ghp_made_up"), "{said}");
    assert_eq!(frame::reply_id(line), Some(7), "so the call it was for ends");
}
