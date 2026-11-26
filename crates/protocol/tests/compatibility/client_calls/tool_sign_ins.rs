//! Signing in to a tool server, as the requests, their replies and the server's record carry it, spelled out.

use protocol::client::tool_sign_ins::{
    NoToolSignIn, ToolSignInBegun, ToolSignInCancel, ToolSignInFinish, ToolSignInPage, ToolSignInReturned,
    ToolSignInStart, ToolSignInTo, redirect,
};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{ToolServerAuth, ToolSignInStatus};

use super::{answered, asked};

#[test]
fn every_request_about_a_sign_in_to_a_tool_server_keeps_its_form() {
    let start = |to| ToolSignInStart { id: "i".into(), to, redirect: redirect(17867) };
    let new = ToolSignInTo::New { id: "t".into(), name: "Docs".into(), url: "https://mcp.example.com".into() };
    let returned = ToolSignInReturned::new("code=made-up&state=made-up");

    asked(
        ClientRequest::ToolSignInStart(start(new)),
        r#"{"method":"tool_sign_in_start","id":"i","to":{"kind":"new","id":"t","name":"Docs","url":"https://mcp.example.com"},"redirect":"http://127.0.0.1:17867/callback"}"#,
    );
    asked(
        ClientRequest::ToolSignInStart(start(ToolSignInTo::Again { id: "t".into() })),
        r#"{"method":"tool_sign_in_start","id":"i","to":{"kind":"again","id":"t"},"redirect":"http://127.0.0.1:17867/callback"}"#,
    );
    let edit = |name: Option<&str>, url: Option<&str>| {
        let (name, url) = (name.map(str::to_string), url.map(str::to_string));
        ClientRequest::ToolSignInStart(start(ToolSignInTo::Edit { id: "t".into(), name, url }))
    };
    for (edit, changes) in [
        (edit(None, None), ""),
        (edit(Some("Sentry"), None), r#","name":"Sentry""#),
        (edit(None, Some("https://mcp.example.com")), r#","url":"https://mcp.example.com""#),
    ] {
        let form = format!(
            r#"{{"method":"tool_sign_in_start","id":"i","to":{{"kind":"edit","id":"t"{changes}}},"redirect":"http://127.0.0.1:17867/callback"}}"#
        );
        asked(edit, &form);
    }
    asked(
        ClientRequest::ToolSignInFinish(ToolSignInFinish { id: "i".into(), returned }),
        r#"{"method":"tool_sign_in_finish","id":"i","returned":"code=made-up&state=made-up"}"#,
    );
    asked(
        ClientRequest::ToolSignInCancel(ToolSignInCancel { id: "i".into() }),
        r#"{"method":"tool_sign_in_cancel","id":"i"}"#,
    );
}

#[test]
fn what_a_start_comes_to_keeps_its_form() {
    let issuer = || "https://auth.example.com".to_string();
    let no_way = |why| ToolSignInBegun::NoWay { why };

    for (begun, form) in [
        (
            ToolSignInBegun::Page { page: ToolSignInPage::new("https://auth.example.com/?state=made-up") },
            r#""kind":"page","page":"https://auth.example.com/?state=made-up""#,
        ),
        (ToolSignInBegun::NoSignIn, r#""kind":"no_sign_in""#),
        (no_way(NoToolSignIn::NoAuthorizationServer), r#""kind":"no_way","why":{"kind":"no_authorization_server"}"#),
        (
            no_way(NoToolSignIn::OnlyRegisteredClients { authorization_server: issuer() }),
            r#""kind":"no_way","why":{"kind":"only_registered_clients","authorization_server":"https://auth.example.com"}"#,
        ),
        (
            no_way(NoToolSignIn::OnlyClientsWithSecrets { authorization_server: issuer() }),
            r#""kind":"no_way","why":{"kind":"only_clients_with_secrets","authorization_server":"https://auth.example.com"}"#,
        ),
        (
            no_way(NoToolSignIn::NotRegistered { authorization_server: issuer(), error: None }),
            r#""kind":"no_way","why":{"kind":"not_registered","authorization_server":"https://auth.example.com"}"#,
        ),
        (
            no_way(NoToolSignIn::NotRegistered {
                authorization_server: issuer(),
                error: Some("invalid_redirect_uri".into()),
            }),
            r#""kind":"no_way","why":{"kind":"not_registered","authorization_server":"https://auth.example.com","error":"invalid_redirect_uri"}"#,
        ),
        (
            no_way(NoToolSignIn::OtherServer { said: "https://other.example".into() }),
            r#""kind":"no_way","why":{"kind":"other_server","said":"https://other.example"}"#,
        ),
        (
            no_way(NoToolSignIn::NoS256 { authorization_server: issuer() }),
            r#""kind":"no_way","why":{"kind":"no_s256","authorization_server":"https://auth.example.com"}"#,
        ),
    ] {
        answered(ClientResponse::ToolSignInBegun(begun), &format!(r#"{{"result":"tool_sign_in_begun",{form}}}"#));
    }
}

#[test]
fn a_signed_in_servers_way_of_knowing_the_person_keeps_its_form() {
    for (status, form) in [
        (ToolSignInStatus::Works, r#"{"kind":"sign_in","status":"works"}"#),
        (ToolSignInStatus::NeedsSignIn, r#"{"kind":"sign_in","status":"needs_sign_in"}"#),
        (ToolSignInStatus::NeedsMoreAccess, r#"{"kind":"sign_in","status":"needs_more_access"}"#),
    ] {
        let auth = ToolServerAuth::SignIn { status };

        assert_eq!(serde_json::to_string(&auth).unwrap(), form);
        assert_eq!(serde_json::from_str::<ToolServerAuth>(form).unwrap(), auth);
    }
}

/// From a newer hub, and a newer client.
#[test]
fn what_this_build_doesnt_know_of_a_sign_in_reads_as_unknown() {
    let start = r#"{"method":"tool_sign_in_start","id":"i","to":{"kind":"made_up","id":"t"},"redirect":"u"}"#;
    let begun = r#"{"result":"tool_sign_in_begun","kind":"made_up","made_up":1}"#;
    let why = r#"{"kind":"made_up","made_up":1}"#;
    let status = r#"{"kind":"sign_in","status":"made_up"}"#;

    let ClientRequest::ToolSignInStart(start) = serde_json::from_str(start).unwrap() else { panic!("another request") };
    assert_eq!(start.to, ToolSignInTo::Unknown);
    assert_eq!(
        serde_json::from_str::<ClientResponse>(begun).unwrap(),
        ClientResponse::ToolSignInBegun(ToolSignInBegun::Unknown)
    );
    assert_eq!(serde_json::from_str::<NoToolSignIn>(why).unwrap(), NoToolSignIn::Unknown);
    assert_eq!(
        serde_json::from_str::<ToolServerAuth>(status).unwrap(),
        ToolServerAuth::SignIn { status: ToolSignInStatus::Unknown }
    );
}
