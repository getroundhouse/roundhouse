//! Where each repository is, spelled out: its record, as the feed and a list carry it, with its checkout beside it for
//! a client from before addresses, which reads only that, and its link, and the request that reads a checkout's address and its
//! reply, with where its pushes go, since a worker can outlive the hub that started it.

use protocol::client::HubNotice;
use protocol::frame;
use protocol::model::{AddressForm, GithubRepositoryName, Linking, Repo, RepoLink, RepoOrigin, RepoUrl, Rev, Unlinked};
use protocol::worker::{
    PushRoute, ReadRepoUrl, RepoUrlReply, WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};

use super::keeps_its_form;
use super::worker_calls::{answered, asked};

fn roundhouse() -> RepoUrl {
    RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap()
}

fn repo(origin: RepoOrigin) -> HubNotice {
    linked(origin, Linking::NoAddress)
}

fn linked(origin: RepoOrigin, link: Linking) -> HubNotice {
    HubNotice::Repo(Repo { id: "r".into(), name: "repo".into(), origin, link, created_at_ms: 1, rev: Rev::new(2) })
}

#[test]
fn a_repository_keeps_its_form_wherever_it_is_with_its_checkout_beside_it() {
    let url = r#""url":"https://github.com/ada/roundhouse.git""#;

    keeps_its_form(
        &repo(RepoOrigin::Checkout { path: "/repo".into() }),
        r#"{"notice":"repo","id":"r","name":"repo","path":"/repo","origin":{"kind":"checkout","path":"/repo"},"link":{"state":"no_address"},"created_at_ms":1,"rev":2}"#,
    );
    keeps_its_form(
        &repo(RepoOrigin::Linked { path: "/repo".into(), url: roundhouse() }),
        &format!(
            r#"{{"notice":"repo","id":"r","name":"repo","path":"/repo","origin":{{"kind":"linked","path":"/repo",{url}}},"link":{{"state":"no_address"}},"created_at_ms":1,"rev":2}}"#
        ),
    );
    // With no checkout, a client from before addresses still reads it, with a checkout it can't use.
    keeps_its_form(
        &repo(RepoOrigin::Remote { url: roundhouse() }),
        &format!(
            r#"{{"notice":"repo","id":"r","name":"repo","path":"","origin":{{"kind":"remote",{url}}},"link":{{"state":"no_address"}},"created_at_ms":1,"rev":2}}"#
        ),
    );
}

#[test]
fn a_repository_from_a_hub_from_before_addresses_is_its_checkout() {
    let older = br#"{"notice":"repo","id":"r","name":"repo","path":"/repo","created_at_ms":1,"rev":2}"#;

    let notice: HubNotice = frame::decode(older).unwrap();

    assert_eq!(notice, linked(RepoOrigin::Checkout { path: "/repo".into() }, Linking::Unknown));
}

/// Its link, of each kind, and why one on github.com isn't linked, each as the feed carries it beside `origin`.
#[test]
fn a_repositorys_link_keeps_its_form_of_each_kind() {
    let origin = || RepoOrigin::Linked { path: "/repo".into(), url: roundhouse() };
    let form = |link: &str| {
        format!(
            r#"{{"notice":"repo","id":"r","name":"repo","path":"/repo","origin":{{"kind":"linked","path":"/repo","url":"https://github.com/ada/roundhouse.git"}},"link":{link},"created_at_ms":1,"rev":2}}"#
        )
    };
    let github = RepoLink::Github { id: "R_kgDOJbBqvw".into(), by: AddressForm::Ssh };
    let plain = RepoLink::Plain { url: roundhouse() };
    let not_linked = |why| Linking::NotLinked { repository: GithubRepositoryName::named("ada/roundhouse"), why };

    keeps_its_form(
        &linked(origin(), Linking::Linked { link: github }),
        &form(r#"{"state":"linked","link":{"host":"github","id":"R_kgDOJbBqvw","by":"ssh"}}"#),
    );
    keeps_its_form(
        &linked(origin(), Linking::Linked { link: plain }),
        &form(r#"{"state":"linked","link":{"host":"plain","url":"https://github.com/ada/roundhouse.git"}}"#),
    );
    keeps_its_form(
        &linked(origin(), not_linked(Unlinked::NotFound)),
        &form(r#"{"state":"not_linked","repository":"ada/roundhouse","why":{"reason":"not_found"}}"#),
    );
    keeps_its_form(
        &linked(origin(), not_linked(Unlinked::CouldntTell { why: "offline".into() })),
        &form(
            r#"{"state":"not_linked","repository":"ada/roundhouse","why":{"reason":"couldnt_tell","why":"offline"}}"#,
        ),
    );
}

/// From a newer hub, which may link to a host, or say why it isn't linked, in a way this build has no word for.
#[test]
fn a_link_to_a_host_this_build_does_not_know_still_reads() {
    let newer = br#"{"notice":"repo","id":"r","name":"repo","path":"/repo","origin":{"kind":"checkout","path":"/repo"},"link":{"state":"linked","link":{"host":"gitlab","id":7}},"created_at_ms":1,"rev":2}"#;
    let why = br#"{"notice":"repo","id":"r","name":"repo","path":"/repo","origin":{"kind":"checkout","path":"/repo"},"link":{"state":"not_linked","repository":"ada/roundhouse","why":{"reason":"suspended"}},"created_at_ms":1,"rev":2}"#;

    let HubNotice::Repo(newer) = frame::decode(newer).unwrap() else { panic!() };
    let HubNotice::Repo(why) = frame::decode(why).unwrap() else { panic!() };

    assert_eq!(newer.link, Linking::Linked { link: RepoLink::Unknown });
    assert!(matches!(why.link, Linking::NotLinked { why: Unlinked::Unknown, .. }), "{:?}", why.link);
}

/// From a newer hub, which may have somewhere this build has no word for.
#[test]
fn a_repository_somewhere_this_build_does_not_know_still_reads() {
    let newer = br#"{"notice":"repo","id":"r","name":"repo","path":"","origin":{"kind":"mirrored","from":"x"},"created_at_ms":1,"rev":2}"#;

    let notice: HubNotice = frame::decode(newer).unwrap();

    assert_eq!(notice, linked(RepoOrigin::Unknown, Linking::Unknown));
}

#[test]
fn reading_a_checkouts_address_keeps_its_form() {
    let reply = |url, push| WorkerResponse::RepoUrl(RepoUrlReply { url, push });
    let address = r#""url":"https://github.com/ada/roundhouse.git""#;

    asked(
        WorkerRequest::ReadRepoUrl(ReadRepoUrl { path: "/repo".into() }),
        r#"{"method":"read_repo_url","path":"/repo"}"#,
    );
    let through = reply(Some(roundhouse()), Some(PushRoute::Credential));
    answered(through, &format!(r#"{{"result":"repo_url",{address},"push":"credential"}}"#));
    answered(
        reply(Some(roundhouse()), Some(PushRoute::Around)),
        &format!(r#"{{"result":"repo_url",{address},"push":"around"}}"#),
    );
    answered(reply(None, None), r#"{"result":"repo_url","url":null,"push":null}"#);
}

/// A worker from before it said where pushes go says nothing of it, and a newer one's way is one this build can't
/// name, either of which the hub keeps as no word of it.
#[test]
fn a_reply_that_says_nothing_of_where_pushes_go_or_a_way_this_build_doesnt_know_still_reads() {
    let line = |push: &str| {
        format!(
            r#"{{"frame":"reply","id":7,"response":{{"result":"repo_url","url":"https://github.com/ada/roundhouse.git"{push}}}}}"#
        )
    };
    let push = |line: String| match frame::decode::<WorkerFrame>(line.as_bytes()).unwrap() {
        WorkerFrame::Reply { outcome: WorkerOutcome::Response(WorkerResponse::RepoUrl(reply)), .. } => reply.push,
        other => panic!("{other:?}"),
    };

    assert_eq!(push(line("")), None);
    assert_eq!(push(line(r#","push":"over_a_tunnel""#)), Some(PushRoute::Unknown));
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
