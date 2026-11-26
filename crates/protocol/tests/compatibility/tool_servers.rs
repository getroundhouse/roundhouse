//! A tool server as a hub sends it to a worker, and what a worker asks about one that refused a call.

use protocol::model::{ToolServerName, ToolServerUrl};
use protocol::worker::{
    Ask as _, AskReply, ProxiedServer, RenewToolServer, ServerHeader, ServerSignIn, TokenNumber, WorkerAsk,
};

use super::keeps_its_form;

const SERVER: &str = r#""id":"t","name":"Docs","label":"Docs","url":"https://mcp.example.com/mcp""#;

fn server(header: Option<ServerHeader>, sign_in: Option<ServerSignIn>) -> ProxiedServer {
    ProxiedServer {
        id: "t".into(),
        name: "Docs".into(),
        label: ToolServerName::typed("Docs").unwrap(),
        url: ToolServerUrl::typed("https://mcp.example.com/mcp").unwrap(),
        header,
        sign_in,
    }
}

#[test]
fn a_server_keeps_its_form_signed_in_to_or_not() {
    let works = ServerSignIn::Works { token: TokenNumber::FIRST.next() };
    let header = r#""header":{"name":"Authorization","value":"Bearer made-up"}"#;

    keeps_its_form(&server(None, None), &format!("{{{SERVER}}}"));
    keeps_its_form(
        &server(Some(ServerHeader::bearer("made-up")), Some(works)),
        &format!(r#"{{{SERVER},{header},"sign_in":{{"status":"works","token":2}}}}"#),
    );
    keeps_its_form(
        &server(None, Some(ServerSignIn::NeedsSignIn)),
        &format!(r#"{{{SERVER},"sign_in":{{"status":"needs_sign_in"}}}}"#),
    );
}

#[test]
fn a_server_whose_sign_in_stands_as_only_a_newer_hub_knows_reads_with_the_rest() {
    let newer = format!(r#"{{{SERVER},"sign_in":{{"status":"made_up","with":1}}}}"#);

    let read: ProxiedServer = serde_json::from_str(&newer).unwrap();

    assert_eq!(read, server(None, Some(ServerSignIn::Unknown)));
}

#[test]
fn asking_for_another_access_token_keeps_its_form_and_so_does_its_answer() {
    let asked = RenewToolServer { server: "t".into(), refused: TokenNumber::FIRST };

    keeps_its_form(&asked.into_ask(), r#"{"method":"renew_tool_server","server":"t","refused":1}"#);
    let newer = r#"{"method":"renew_tool_server","server":"t","refused":1,"newer":2}"#;
    assert!(matches!(serde_json::from_str(newer).unwrap(), WorkerAsk::RenewToolServer(_)));
    let answer = AskReply::ToolServer(server(None, Some(ServerSignIn::NeedsSignIn)));
    keeps_its_form(&answer, &format!(r#"{{"result":"tool_server",{SERVER},"sign_in":{{"status":"needs_sign_in"}}}}"#));
}
