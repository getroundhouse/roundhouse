//! The git credential a hub sets on its worker, spelled out, since a worker can outlive the hub that started it, and
//! the next hub can be of another build, newer or older.

use protocol::frame;
use protocol::model::GithubToken;
use protocol::worker::{self, GithubCredential, WorkerRequest};

use super::worker_calls::asked;

/// An app's installation, whose commits are by its bot.
const BOT: &str = r#"{"kind":"account","username":"x-access-token","token":"made-up","identity":{"name":"ada-roundhouse[bot]","email":"41+ada-roundhouse[bot]@users.noreply.github.com"}}"#;

fn bot() -> worker::GitIdentity {
    worker::GitIdentity {
        name: "ada-roundhouse[bot]".to_string(),
        email: "41+ada-roundhouse[bot]@users.noreply.github.com".to_string(),
    }
}

#[test]
fn every_git_credential_keeps_its_form() {
    let set = |credential| WorkerRequest::GitCredentialSet(worker::GitCredentialSet { credential });
    let ada = worker::GitCredential { username: "ada".into(), token: GithubToken::new("made-up"), identity: None };
    let installation = worker::GitCredential {
        username: "x-access-token".into(),
        token: GithubToken::new("made-up"),
        identity: Some(bot()),
    };
    let renew = "Renew ada in Settings".to_string();

    asked(set(GithubCredential::MachineOwn), r#"{"method":"git_credential_set","credential":{"kind":"machine_own"}}"#);
    asked(
        set(GithubCredential::Account(ada)),
        r#"{"method":"git_credential_set","credential":{"kind":"account","username":"ada","token":"made-up"}}"#,
    );
    asked(
        set(GithubCredential::Account(installation)),
        &format!(r#"{{"method":"git_credential_set","credential":{BOT}}}"#),
    );
    asked(
        set(GithubCredential::Blocked { reason: renew }),
        r#"{"method":"git_credential_set","credential":{"kind":"blocked","reason":"Renew ada in Settings"}}"#,
    );
}

/// From a newer hub, which a worker treats as blocked rather than as one it knows.
#[test]
fn a_git_credential_of_a_kind_this_build_doesnt_know_reads_as_unknown() {
    let line = r#"{"method":"git_credential_set","credential":{"kind":"app","installation":9,"token":"made-up"}}"#;

    let read = frame::decode::<WorkerRequest>(line.as_bytes()).unwrap();

    assert_eq!(
        read,
        WorkerRequest::GitCredentialSet(worker::GitCredentialSet { credential: GithubCredential::Unknown })
    );
}

#[test]
fn a_credential_that_names_no_identity_has_none() {
    let line = r#"{"kind":"account","username":"ada","token":"made-up"}"#;

    let read: GithubCredential = serde_json::from_str(line).unwrap();

    let GithubCredential::Account(credential) = read else { panic!("{read:?}") };
    assert_eq!(credential.identity, None);
}
