//! Saving a tool server once the hub has asked it for its tools, and cancelling that, as the requests and the reply
//! carry them, spelled out.

use protocol::client::tool_server_checks::{NoToolList, ToolServerToCheck};
use protocol::client::tool_server_saves::{SaveWhen, ToolServerSave, ToolServerSaveCancel, ToolServerSaved};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{
    Rev, ToolServer, ToolServerAuthEdit, ToolServerAuthGiven, ToolServerKey, ToolServerName, ToolServerUrl,
};

use super::{answered, asked};

fn save(of: ToolServerToCheck, when: SaveWhen) -> ClientRequest {
    ClientRequest::ToolServerSave(ToolServerSave { id: "s".into(), of, when })
}

/// What's to be saved is held as the request it would be on its own is, and a check of it.
#[test]
fn a_save_of_a_tool_server_holds_what_would_be_done_and_when_and_keeps_its_form() {
    let url = || "https://mcp.example.com/mcp".to_string();
    let given = ToolServerAuthGiven::Key { header: "X-Key".into(), key: ToolServerKey::new("made-up") };
    let add = ToolServerAdd { id: "t".into(), name: "Docs".into(), url: url(), auth: given };
    let auth = ToolServerAuthEdit::Key { header: "X-Key".into(), key: None, url: url() };
    let edit = ToolServerEdit { id: "t".into(), name: None, url: Some(url()), auth: Some(auth), saved_anyway: false };

    asked(
        save(ToolServerToCheck::Add(add), SaveWhen::Listed),
        r#"{"method":"tool_server_save","id":"s","of":{"kind":"add","id":"t","name":"Docs","url":"https://mcp.example.com/mcp","auth":{"kind":"key","header":"X-Key","key":"made-up"}},"when":"listed"}"#,
    );
    asked(
        save(ToolServerToCheck::Edit(edit), SaveWhen::Anyway),
        r#"{"method":"tool_server_save","id":"s","of":{"kind":"edit","id":"t","url":"https://mcp.example.com/mcp","auth":{"kind":"key","header":"X-Key","url":"https://mcp.example.com/mcp"}},"when":"anyway"}"#,
    );
    asked(
        ClientRequest::ToolServerSaveCancel(ToolServerSaveCancel { id: "s".into() }),
        r#"{"method":"tool_server_save_cancel","id":"s"}"#,
    );
}

#[test]
fn what_a_save_comes_to_keeps_its_form() {
    let server = ToolServer {
        name: ToolServerName::typed("Docs").unwrap(),
        url: ToolServerUrl::typed("https://mcp.example.com/mcp").unwrap(),
        rev: Rev::new(3),
        ..protocol::fixtures::tool_server("t")
    };
    let saved = |saved| ClientResponse::ToolServerSaved(saved);

    answered(
        saved(ToolServerSaved::Saved { server }),
        r#"{"result":"tool_server_saved","kind":"saved","server":{"id":"t","name":"Docs","url":"https://mcp.example.com/mcp","auth":{"kind":"key","header":"Authorization"},"rev":3}}"#,
    );
    answered(
        saved(ToolServerSaved::NotListed { why: NoToolList::KeyRefused { status: 401 } }),
        r#"{"result":"tool_server_saved","kind":"not_listed","why":{"kind":"key_refused","status":401}}"#,
    );
    answered(saved(ToolServerSaved::Cancelled), r#"{"result":"tool_server_saved","kind":"cancelled"}"#);
}

/// From a newer client, and a newer hub.
#[test]
fn what_a_save_holds_that_this_build_doesnt_know_reads_as_unknown() {
    let when = r#"{"method":"tool_server_save","id":"s","of":{"kind":"try"},"when":"later"}"#;
    let saved = r#"{"kind":"put_off","made_up":1}"#;

    let ClientRequest::ToolServerSave(save) = serde_json::from_str(when).unwrap() else { panic!() };
    assert_eq!((save.of, save.when), (ToolServerToCheck::Unknown, SaveWhen::Unknown));
    assert_eq!(serde_json::from_str::<ToolServerSaved>(saved).unwrap(), ToolServerSaved::Unknown);
}
