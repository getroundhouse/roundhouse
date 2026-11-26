//! Checking a tool server, as the request and its reply carry it, spelled out.

use protocol::client::tool_server_checks::{
    NoToolList, ToolServerCheck, ToolServerChecked, ToolServerToCheck, ToolServerUnreached,
};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey};

use super::{answered, asked};

fn check(of: ToolServerToCheck) -> ClientRequest {
    ClientRequest::ToolServerCheck(ToolServerCheck { of })
}

/// Each holds the request it's a check of as that request does.
#[test]
fn a_check_of_a_tool_server_holds_what_would_be_done_and_keeps_its_form() {
    let key = || ToolServerKey::new("made-up");
    let given = ToolServerAuthGiven::Key { header: "X-Key".into(), key: key() };
    let url = || "https://mcp.example.com/mcp".to_string();
    let add = ToolServerAdd { id: "t".into(), name: "Docs".into(), url: url(), auth: given };
    let auth = ToolServerAuthEdit::Key { header: "X-Key".into(), key: Some(key()), url: url() };
    let edit = ToolServerEdit { id: "t".into(), name: None, url: Some(url()), auth: Some(auth) };

    asked(
        check(ToolServerToCheck::Add(add)),
        r#"{"method":"tool_server_check","of":{"kind":"add","id":"t","name":"Docs","url":"https://mcp.example.com/mcp","auth":{"kind":"key","header":"X-Key","key":"made-up"}}}"#,
    );
    asked(
        check(ToolServerToCheck::Edit(edit)),
        r#"{"method":"tool_server_check","of":{"kind":"edit","id":"t","url":"https://mcp.example.com/mcp","auth":{"kind":"key","header":"X-Key","key":"made-up","url":"https://mcp.example.com/mcp"}}}"#,
    );
}

#[test]
fn what_a_check_comes_to_keeps_its_form() {
    let not_listed = |why| ToolServerChecked::NotListed { why };
    let unreached = |why| not_listed(NoToolList::Unreached { why });
    let because = |why| unreached(Some(why));
    let why = |said: &str| format!(r#""kind":"not_listed","why":{{"kind":{said}}}"#);

    for (checked, form) in [
        (ToolServerChecked::NothingToAsk, r#""kind":"nothing_to_ask""#.to_string()),
        (ToolServerChecked::Listed { tools: 7 }, r#""kind":"listed","tools":7"#.to_string()),
        (unreached(None), why(r#""unreached""#)),
        (because(ToolServerUnreached::NoSuchName), why(r#""unreached","why":"no_such_name""#)),
        (because(ToolServerUnreached::ConnectionRefused), why(r#""unreached","why":"connection_refused""#)),
        (because(ToolServerUnreached::NoRoute), why(r#""unreached","why":"no_route""#)),
        (because(ToolServerUnreached::NotTrusted), why(r#""unreached","why":"not_trusted""#)),
        (because(ToolServerUnreached::NotHttps), why(r#""unreached","why":"not_https""#)),
        (because(ToolServerUnreached::NoCertificates), why(r#""unreached","why":"no_certificates""#)),
        (not_listed(NoToolList::TimedOut), why(r#""timed_out""#)),
        (not_listed(NoToolList::Lost), why(r#""lost""#)),
        (not_listed(NoToolList::Redirected), why(r#""redirected""#)),
        (not_listed(NoToolList::KeyRefused { status: 403 }), why(r#""key_refused","status":403"#)),
        (not_listed(NoToolList::WantsSignInOrKey), why(r#""wants_sign_in_or_key""#)),
        (not_listed(NoToolList::Forbidden), why(r#""forbidden""#)),
        (not_listed(NoToolList::NotFound), why(r#""not_found""#)),
        (not_listed(NoToolList::TooManyRequests), why(r#""too_many_requests""#)),
        (not_listed(NoToolList::Failing { status: 503 }), why(r#""failing","status":503"#)),
        (not_listed(NoToolList::NotMcp), why(r#""not_mcp""#)),
        (not_listed(NoToolList::TooLong), why(r#""too_long""#)),
        (not_listed(NoToolList::WontConnect), why(r#""wont_connect""#)),
        (not_listed(NoToolList::WontList), why(r#""wont_list""#)),
    ] {
        answered(ClientResponse::ToolServerChecked(checked), &format!(r#"{{"result":"tool_server_checked",{form}}}"#));
    }
}

/// From a newer client, and a newer hub.
#[test]
fn what_a_check_holds_that_this_build_doesnt_know_reads_as_unknown() {
    let of = r#"{"kind":"try","made_up":1}"#;
    let why = r#"{"kind":"not_listed","why":{"kind":"asleep","made_up":1}}"#;
    let unreached = r#"{"kind":"unreached","why":"asleep"}"#;

    assert_eq!(serde_json::from_str::<ToolServerToCheck>(of).unwrap(), ToolServerToCheck::Unknown);
    assert_eq!(serde_json::from_str::<ToolServerChecked>(of).unwrap(), ToolServerChecked::Unknown);
    assert_eq!(
        serde_json::from_str::<ToolServerChecked>(why).unwrap(),
        ToolServerChecked::NotListed { why: NoToolList::Unknown }
    );
    assert_eq!(
        serde_json::from_str::<NoToolList>(unreached).unwrap(),
        NoToolList::Unreached { why: Some(ToolServerUnreached::Unknown) }
    );
}
