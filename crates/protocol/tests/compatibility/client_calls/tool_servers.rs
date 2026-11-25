//! The tool servers a hub has of its own, as the requests, replies, feed and lists carry them, spelled out.

use protocol::client::tool_servers::{ToolServerAdd, ToolServerRemove};
use protocol::client::{ClientRequest, ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{
    RecordKey, Records, Rev, ToolServer, ToolServerAuth, ToolServerAuthGiven, ToolServerKey, ToolServerName,
    ToolServerUrl,
};

use super::{answered, asked};

const WITH_KEY: &str = concat!(
    r#""id":"t","name":"Linear (Northwind)","url":"https://mcp.example.com/mcp","#,
    r#""auth":{"kind":"key","header":"Authorization"},"rev":3"#,
);

fn with_key() -> ToolServer {
    ToolServer {
        name: ToolServerName::typed("Linear (Northwind)").unwrap(),
        url: ToolServerUrl::typed("https://mcp.example.com/mcp").unwrap(),
        rev: Rev::new(3),
        ..protocol::fixtures::tool_server("t")
    }
}

#[test]
fn every_request_about_a_tool_server_keeps_its_form() {
    let add = |auth| ToolServerAdd {
        id: "t".into(),
        name: "Linear (Northwind)".to_string(),
        url: "https://mcp.example.com/mcp".to_string(),
        auth,
    };
    let key = ToolServerAuthGiven::Key { header: "Authorization".to_string(), key: ToolServerKey::new("made-up") };

    asked(
        ClientRequest::ToolServerAdd(add(key)),
        r#"{"method":"tool_server_add","id":"t","name":"Linear (Northwind)","url":"https://mcp.example.com/mcp","auth":{"kind":"key","header":"Authorization","key":"made-up"}}"#,
    );
    asked(
        ClientRequest::ToolServerAdd(add(ToolServerAuthGiven::Neither)),
        r#"{"method":"tool_server_add","id":"t","name":"Linear (Northwind)","url":"https://mcp.example.com/mcp","auth":{"kind":"neither"}}"#,
    );
    asked(
        ClientRequest::ToolServerRemove(ToolServerRemove { id: "t".into() }),
        r#"{"method":"tool_server_remove","id":"t"}"#,
    );
}

#[test]
fn a_tool_servers_record_keeps_its_form_as_a_reply() {
    let neither = ToolServer { auth: ToolServerAuth::Neither, ..with_key() };

    answered(ClientResponse::ToolServer(with_key()), &format!(r#"{{"result":"tool_server",{WITH_KEY}}}"#));
    answered(
        ClientResponse::ToolServer(neither),
        r#"{"result":"tool_server","id":"t","name":"Linear (Northwind)","url":"https://mcp.example.com/mcp","auth":{"kind":"neither"},"rev":3}"#,
    );
}

#[test]
fn a_notice_of_a_tool_server_reads_as_its_record_and_its_removal_by_its_key_and_a_list_holds_it() {
    let line = format!(r#"{{"notice":"tool_server",{WITH_KEY}}}"#);
    let removed = r#"{"notice":"removed","record":{"kind":"tool_server","id":"t"},"rev":4}"#;
    let list = Records { tool_servers: vec![with_key()], ..Records::default() };

    let notice = frame::decode::<HubNotice>(line.as_bytes()).unwrap();
    let removal = frame::decode::<HubNotice>(removed.as_bytes()).unwrap();
    let listed = serde_json::to_string(&list).unwrap();

    assert_eq!(notice, HubNotice::ToolServer(with_key()));
    assert_eq!(removal, HubNotice::Removed { record: RecordKey::ToolServer { id: "t".into() }, rev: Rev::new(4) });
    assert!(listed.contains(&format!(r#""tool_servers":[{{{WITH_KEY}}}]"#)), "{listed}");
    assert_eq!(serde_json::from_str::<Records>(&listed).unwrap(), list);
}

/// From a newer hub, and a newer client.
#[test]
fn a_way_of_knowing_the_person_this_build_doesnt_know_reads_as_unknown() {
    let auth = r#"{"kind":"passkey","made_up":1}"#;

    assert_eq!(serde_json::from_str::<ToolServerAuth>(auth).unwrap(), ToolServerAuth::Unknown);
    assert_eq!(serde_json::from_str::<ToolServerAuthGiven>(auth).unwrap(), ToolServerAuthGiven::Unknown);
}
