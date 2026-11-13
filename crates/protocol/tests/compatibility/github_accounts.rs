//! GitHub accounts' records, as the feed and a list carry them, spelled out, since a client of another build finds a
//! record by its kind's name and its key.

use protocol::client::{ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{
    FoundGithubLogin, GithubAccount, GithubLogin, GithubLoginKind, GithubUser, GithubUserId, HeldGithubLogin,
    RecordKey, Rev,
};

const ACCOUNT: &str = r#""id":"g","user":{"id":7,"name":"ada"},"held_login":{"how":"sign_in","connected_at_ms":1,"ends_at_ms":9,"needs_sign_in":true},"rev":2"#;

fn account() -> GithubAccount {
    GithubAccount {
        id: "g".into(),
        user: GithubUser { id: GithubUserId::new(7), name: "ada".into() },
        held_login: HeldGithubLogin {
            how: GithubLoginKind::SignIn,
            connected_at_ms: 1,
            ends_at_ms: Some(9),
            needs_sign_in: true,
        },
        rev: Rev::new(2),
    }
}

fn found(login: Option<&str>) -> FoundGithubLogin {
    FoundGithubLogin {
        provider: "local".into(),
        login: login.map(|username| GithubLogin { username: username.into() }),
        rev: Rev::new(3),
    }
}

#[test]
fn a_notice_of_a_github_account_or_a_found_login_reads_as_its_record() {
    let read = |line: &str| frame::decode::<HubNotice>(line.as_bytes()).unwrap();

    assert_eq!(read(&format!(r#"{{"notice":"github_account",{ACCOUNT}}}"#)), HubNotice::GithubAccount(account()));
    assert_eq!(
        read(r#"{"notice":"found_github_login","provider":"local","login":{"username":"ada"},"rev":3}"#),
        HubNotice::FoundGithubLogin(found(Some("ada")))
    );
    assert_eq!(
        read(r#"{"notice":"found_github_login","provider":"local","login":null,"rev":3}"#),
        HubNotice::FoundGithubLogin(found(None)),
        "asked, and logged in to nothing"
    );
}

#[test]
fn a_github_account_is_removed_by_its_id_and_a_found_login_by_its_provider() {
    let read = |line: &str| frame::decode::<HubNotice>(line.as_bytes()).unwrap();

    let disconnected = r#"{"notice":"removed","record":{"kind":"github_account","id":"g"},"rev":4}"#;
    let account = RecordKey::GithubAccount { id: "g".into() };
    assert_eq!(read(disconnected), HubNotice::Removed { record: account, rev: Rev::new(4) });
    let forgotten = r#"{"notice":"removed","record":{"kind":"found_github_login","provider":"local"},"rev":5}"#;
    let found = RecordKey::FoundGithubLogin { provider: "local".into() };
    assert_eq!(read(forgotten), HubNotice::Removed { record: found, rev: Rev::new(5) });
}

#[test]
fn a_list_of_every_record_reads_its_github_accounts_and_found_logins() {
    let line = format!(
        r#"{{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_accounts":[{{{ACCOUNT}}}],"found_github_logins":[{{"provider":"local","login":{{"username":"ada"}},"rev":3}}],"rev":5}}"#
    );

    let ClientResponse::Records(list) = frame::decode::<ClientResponse>(line.as_bytes()).unwrap() else { panic!() };

    assert_eq!((list.github_accounts, list.found_github_logins), (vec![account()], vec![found(Some("ada"))]));
}
