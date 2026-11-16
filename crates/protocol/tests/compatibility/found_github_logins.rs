//! The logins found on each provider's machines, spelled out: their records, as the feed and a list carry them, since a
//! client of another build finds a record by its kind's name and its key, and the request that finds one and its reply,
//! since a worker can outlive the hub that started it.

use protocol::client::{ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{FoundGithubLogin, GhLogin, GhLoginWorks, RecordKey, Rev};
use protocol::worker::{GithubLoginReply, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};

use super::keeps_its_form;
use super::worker_calls::{answered, asked};

fn ada() -> GhLogin {
    ada_and(GhLoginWorks::Yes)
}

fn ada_and(works: GhLoginWorks) -> GhLogin {
    GhLogin::LoggedIn { username: "ada".into(), works }
}

fn found(login: GhLogin) -> FoundGithubLogin {
    FoundGithubLogin { provider: "local".into(), login, rev: Rev::new(3) }
}

#[test]
fn a_notice_of_each_found_login_keeps_its_form() {
    let notice = |login| HubNotice::FoundGithubLogin(found(login));

    keeps_its_form(
        &notice(ada()),
        r#"{"notice":"found_github_login","provider":"local","login":{"state":"logged_in","username":"ada","works":"yes"},"rev":3}"#,
    );
    keeps_its_form(
        &notice(GhLogin::LoggedOut),
        r#"{"notice":"found_github_login","provider":"local","login":{"state":"logged_out"},"rev":3}"#,
    );
    keeps_its_form(
        &notice(GhLogin::NotInstalled),
        r#"{"notice":"found_github_login","provider":"local","login":{"state":"not_installed"},"rev":3}"#,
    );
}

/// Keyed by its provider, though the hub forgets them as it starts rather than removing one.
#[test]
fn a_found_logins_removal_names_its_provider() {
    let removed =
        HubNotice::Removed { record: RecordKey::FoundGithubLogin { provider: "local".into() }, rev: Rev::new(5) };

    keeps_its_form(
        &removed,
        r#"{"notice":"removed","record":{"kind":"found_github_login","provider":"local"},"rev":5}"#,
    );
}

#[test]
fn a_list_of_every_record_reads_its_found_logins_and_one_from_a_hub_that_finds_none_has_none() {
    let listed = r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"found_github_logins":[{"provider":"local","login":{"state":"logged_in","username":"ada","works":"yes"},"rev":3}],"rev":5}"#;
    let older = r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":5}"#;
    let read = |line: &str| {
        let ClientResponse::Records(list) = frame::decode::<ClientResponse>(line.as_bytes()).unwrap() else { panic!() };
        list.found_github_logins
    };

    assert_eq!(read(listed), [found(ada())]);
    assert_eq!(read(older), []);
}

#[test]
fn finding_the_machines_own_login_keeps_its_form() {
    let reply = |login| WorkerResponse::GithubLogin(GithubLoginReply { login });

    asked(WorkerRequest::FindGithubLogin, r#"{"method":"find_github_login"}"#);
    let logged_in = |works| {
        format!(r#"{{"result":"github_login","login":{{"state":"logged_in","username":"ada","works":"{works}"}}}}"#)
    };
    answered(reply(ada()), &logged_in("yes"));
    answered(reply(ada_and(GhLoginWorks::Refused)), &logged_in("refused"));
    answered(reply(ada_and(GhLoginWorks::CouldntTell)), &logged_in("couldnt_tell"));
    answered(reply(GhLogin::LoggedOut), r#"{"result":"github_login","login":{"state":"logged_out"}}"#);
    answered(reply(GhLogin::NotInstalled), r#"{"result":"github_login","login":{"state":"not_installed"}}"#);
}

/// From a newer hub, which may ask with more, and a newer worker, which may find what this build has no word for.
#[test]
fn finding_one_reads_with_what_a_newer_peer_adds() {
    let request = r#"{"method":"find_github_login","newer":1}"#;
    let reply = r#"{"frame":"reply","id":7,"response":{"result":"github_login","login":{"state":"signed_in_twice"}}}"#;

    let request = frame::decode::<WorkerRequest>(request.as_bytes()).unwrap();
    let reply = frame::decode::<WorkerFrame>(reply.as_bytes()).unwrap();

    assert_eq!(request, WorkerRequest::FindGithubLogin);
    let unknown = WorkerResponse::GithubLogin(GithubLoginReply { login: GhLogin::Unknown });
    assert_eq!(reply, WorkerFrame::Reply { id: 7, outcome: WorkerOutcome::Response(unknown) });
}

/// From a newer worker, which may say whether a token works in a way this build has no word for.
#[test]
fn a_login_that_works_in_a_newer_way_reads_as_not_said() {
    let newer = r#"{"result":"github_login","login":{"state":"logged_in","username":"ada","works":"for_now"}}"#;

    let reply = frame::decode::<WorkerResponse>(newer.as_bytes()).unwrap();

    let login = ada_and(GhLoginWorks::Unknown);
    assert_eq!(reply, WorkerResponse::GithubLogin(GithubLoginReply { login }));
}
