//! GitHub accounts' records, as the feed and a list carry them, spelled out, since a client of another build finds a
//! record by its kind's name and its key.

use protocol::client::{ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{GithubAccount, GithubLoginKind, GithubUser, GithubUserId, HeldGithubLogin, Rev};

const ACCOUNT: &str = r#""id":"g","user":{"id":7,"name":"ada"},"held_login":{"how":"token","connected_at_ms":1,"ends_at_ms":null},"rev":2"#;

fn account() -> GithubAccount {
    GithubAccount {
        id: "g".into(),
        user: GithubUser { id: GithubUserId::new(7), name: "ada".into() },
        held_login: Some(HeldGithubLogin { how: GithubLoginKind::Token, connected_at_ms: 1, ends_at_ms: None }),
        rev: Rev::new(2),
    }
}

#[test]
fn a_notice_of_a_github_account_reads_as_its_record() {
    let line = format!(r#"{{"notice":"github_account",{ACCOUNT}}}"#);

    let notice = frame::decode::<HubNotice>(line.as_bytes()).unwrap();

    assert_eq!(notice, HubNotice::GithubAccount(account()));
}

#[test]
fn a_list_of_every_record_reads_its_github_accounts_and_one_from_a_hub_that_keeps_none_has_none() {
    let listed = format!(
        r#"{{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_accounts":[{{{ACCOUNT}}}],"rev":5}}"#
    );
    let older = r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":5}"#;
    let read = |line: &str| {
        let ClientResponse::Records(list) = frame::decode::<ClientResponse>(line.as_bytes()).unwrap() else { panic!() };
        list.github_accounts
    };

    assert_eq!(read(&listed), [account()]);
    assert_eq!(read(older), []);
}
