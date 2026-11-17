//! GitHub accounts' and sign-ins' records, as the feed and a list carry them, spelled out, since a client of another
//! build finds a record by its kind's name and its key.

use protocol::client::{ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{
    GithubAccount, GithubLoginKind, GithubSignIn, GithubSignInStatus, GithubUser, GithubUserId, HeldGithubLogin,
    RecordKey, Records, Rev,
};

const ACCOUNT: &str = r#""id":"g","user":{"id":7,"name":"ada"},"held_login":{"how":"token","connected_at_ms":1,"ends_at_ms":null,"needs_sign_in":false},"rev":2"#;

const SIGN_IN: &str = r#""id":"s","status":{"state":"waiting","code":"WDJB-MJHT","address":"https://github.com/login/device","code_ends_at_ms":900000},"rev":3"#;

fn account() -> GithubAccount {
    GithubAccount {
        id: "g".into(),
        user: GithubUser { id: GithubUserId::new(7), name: "ada".into() },
        held_login: Some(HeldGithubLogin {
            how: GithubLoginKind::Token,
            connected_at_ms: 1,
            ends_at_ms: None,
            needs_sign_in: false,
        }),
        installation: None,
        rev: Rev::new(2),
    }
}

fn sign_in() -> GithubSignIn {
    GithubSignIn { rev: Rev::new(3), ..protocol::fixtures::github_sign_in("s") }
}

fn listed(line: &str) -> Records {
    let ClientResponse::Records(list) = frame::decode::<ClientResponse>(line.as_bytes()).unwrap() else { panic!() };
    list
}

#[test]
fn a_notice_of_a_github_account_reads_as_its_record() {
    let line = format!(r#"{{"notice":"github_account",{ACCOUNT}}}"#);

    let notice = frame::decode::<HubNotice>(line.as_bytes()).unwrap();

    assert_eq!(notice, HubNotice::GithubAccount(account()));
}

#[test]
fn a_list_of_every_record_reads_its_github_accounts_and_one_from_a_hub_that_keeps_none_has_none() {
    let line = format!(
        r#"{{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_accounts":[{{{ACCOUNT}}}],"rev":5}}"#
    );
    let older = r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":5}"#;

    assert_eq!(listed(&line).github_accounts, [account()]);
    assert_eq!(listed(older).github_accounts, []);
}

/// A login from a hub from before a sign-in could need signing in again says nothing of it.
#[test]
fn a_signed_in_login_keeps_its_form_and_one_from_a_hub_from_before_needs_no_sign_in() {
    let held =
        HeldGithubLogin { how: GithubLoginKind::SignIn, connected_at_ms: 1, ends_at_ms: Some(9), needs_sign_in: true };
    let older = r#"{"how":"sign_in","connected_at_ms":1,"ends_at_ms":9}"#;

    let written = serde_json::to_string(&held).unwrap();
    let read: HeldGithubLogin = serde_json::from_str(older).unwrap();

    assert_eq!(written, r#"{"how":"sign_in","connected_at_ms":1,"ends_at_ms":9,"needs_sign_in":true}"#);
    assert_eq!(read, HeldGithubLogin { needs_sign_in: false, ..held });
}

#[test]
fn a_notice_of_a_sign_in_reads_as_its_record_and_its_removal_by_its_key() {
    let line = format!(r#"{{"notice":"github_sign_in",{SIGN_IN}}}"#);
    let removed = r#"{"notice":"removed","record":{"kind":"github_sign_in","id":"s"},"rev":4}"#;

    let notice = frame::decode::<HubNotice>(line.as_bytes()).unwrap();
    let removal = frame::decode::<HubNotice>(removed.as_bytes()).unwrap();

    assert_eq!(notice, HubNotice::GithubSignIn(sign_in()));
    assert_eq!(removal, HubNotice::Removed { record: RecordKey::GithubSignIn { id: "s".into() }, rev: Rev::new(4) });
}

#[test]
fn every_state_of_a_sign_in_keeps_its_form() {
    let states = [
        (GithubSignInStatus::Approved { account: "g".into() }, r#"{"state":"approved","account":"g"}"#),
        (GithubSignInStatus::Refused, r#"{"state":"refused"}"#),
        (GithubSignInStatus::Ended, r#"{"state":"ended"}"#),
        (GithubSignInStatus::Cancelled, r#"{"state":"cancelled"}"#),
        (GithubSignInStatus::Failed { message: "made up".to_string() }, r#"{"state":"failed","message":"made up"}"#),
    ];

    for (status, form) in states {
        assert_eq!(serde_json::to_string(&status).unwrap(), form);
        assert_eq!(serde_json::from_str::<GithubSignInStatus>(form).unwrap(), status);
    }
}

#[test]
fn a_list_of_every_record_reads_its_sign_ins_and_one_from_a_hub_that_cant_sign_in_has_none() {
    let line = format!(
        r#"{{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_sign_ins":[{{{SIGN_IN}}}],"rev":5}}"#
    );
    let older =
        r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_accounts":[],"rev":5}"#;

    assert_eq!(listed(&line).github_sign_ins, [sign_in()]);
    assert_eq!(listed(older).github_sign_ins, []);
}
