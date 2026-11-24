//! Apps of your own, and the GitHub accounts of their installations, as the requests, replies, feed and lists carry
//! them, spelled out.

use protocol::client::github_apps::{
    GithubAppInstallationsConnect, GithubAppMake, GithubAppManifest, GithubAppManifestWrite, GithubAppRemove,
    GithubManifestCode,
};
use protocol::client::{ClientRequest, ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{
    GithubAccount, GithubApp, GithubAppId, GithubInstallation, GithubInstallationId, GithubLoginKind, HeldGithubLogin,
    RecordKey, Rev,
};
use serde_json::json;

use super::{answered, asked};

const APP: &str = r#""id":9,"rev":3"#;

fn app() -> GithubApp {
    GithubApp { rev: Rev::new(3), ..protocol::fixtures::github_app(9) }
}

#[test]
fn every_request_about_an_app_keeps_its_form() {
    let write = GithubAppManifestWrite {
        name: "ada-roundhouse".to_string(),
        redirect_url: "http://127.0.0.1:4711/made".to_string(),
    };
    asked(
        ClientRequest::GithubAppManifestWrite(write),
        r#"{"method":"github_app_manifest_write","name":"ada-roundhouse","redirect_url":"http://127.0.0.1:4711/made"}"#,
    );
    asked(
        ClientRequest::GithubAppMake(GithubAppMake { code: GithubManifestCode::new("a-made-up-code") }),
        r#"{"method":"github_app_make","code":"a-made-up-code"}"#,
    );
    let id = GithubAppId::new(9);
    asked(
        ClientRequest::GithubAppInstallationsConnect(GithubAppInstallationsConnect { id }),
        r#"{"method":"github_app_installations_connect","id":9}"#,
    );
    asked(ClientRequest::GithubAppRemove(GithubAppRemove { id }), r#"{"method":"github_app_remove","id":9}"#);
}

#[test]
fn an_apps_record_and_its_manifest_keep_their_forms_as_replies() {
    let manifest = json!({"name": "ada-roundhouse", "public": false});

    answered(ClientResponse::GithubApp(app()), &format!(r#"{{"result":"github_app",{APP}}}"#));
    answered(
        ClientResponse::GithubAppManifest(GithubAppManifest { manifest }),
        r#"{"result":"github_app_manifest","manifest":{"name":"ada-roundhouse","public":false}}"#,
    );
}

/// From the hub of the build before, which still filled what GitHub says of it for a client of version 4.
#[test]
fn a_notice_of_an_app_reads_as_its_record_and_its_removal_by_its_key() {
    let line = r#"{"notice":"github_app","id":9,"slug":"ada-roundhouse","name":"Ada's Roundhouse","owner":{"id":1,"name":"ada"}}"#;
    let removed = r#"{"notice":"removed","record":{"kind":"github_app","id":9},"rev":4}"#;

    let notice = frame::decode::<HubNotice>(line.as_bytes()).unwrap();
    let removal = frame::decode::<HubNotice>(removed.as_bytes()).unwrap();

    assert_eq!(notice, HubNotice::GithubApp(GithubApp { rev: Rev::default(), ..app() }));
    let key = RecordKey::GithubApp { id: GithubAppId::new(9) };
    assert_eq!(removal, HubNotice::Removed { record: key, rev: Rev::new(4) });
}

/// Its user is the app's bot, and its login says it's an app's, with no end.
#[test]
fn an_installations_account_keeps_its_form() {
    let account = GithubAccount {
        id: "g".into(),
        user: protocol::fixtures::github_user(41),
        held_login: Some(HeldGithubLogin {
            how: GithubLoginKind::App,
            connected_at_ms: 1,
            ends_at_ms: None,
            needs_sign_in: false,
        }),
        installation: Some(GithubInstallation {
            app: GithubAppId::new(9),
            id: GithubInstallationId::new(70),
            on: protocol::fixtures::github_user(2),
        }),
        rev: Rev::new(2),
    };

    answered(
        ClientResponse::GithubAccount(account),
        r#"{"result":"github_account","id":"g","user":{"id":41},"held_login":{"how":"app","connected_at_ms":1,"ends_at_ms":null,"needs_sign_in":false},"installation":{"app":9,"id":70,"on":{"id":2}},"rev":2}"#,
    );
}
