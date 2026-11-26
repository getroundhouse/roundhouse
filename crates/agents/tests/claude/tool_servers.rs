//! What the adapter makes of what the CLI says of its tool servers, with small scripts standing in for the CLI.

#![cfg(test)]

use agents::claude::spawn_at;
use agents::{AgentOptions, AgentOutput};
use std::time::Duration;

use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::capability::{Control, Report};
use protocol::event::EventBody;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::process::{script, spec, until_exit};

/// It says nothing the log keeps, so it's beside the event `init` is, and after it.
#[tokio::test]
async fn the_servers_init_lists_are_a_report_after_its_event() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let line = r#"{"type":"system","subtype":"init","session_id":"s","tools":["mcp__docs__find"],"mcp_servers":[{"name":"docs","status":"connected"}]}"#;
    let body = format!("printf '%s\\n' '{line}'");
    spawn_at(script(dir.path(), &body), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();

    let seen = until_exit(&mut rx).await;

    let opened = |item: &AgentOutput| matches!(item, AgentOutput::Event { body: EventBody::AgentInit { .. }, .. });
    let after: Vec<_> = seen.iter().skip_while(|item| !opened(item)).collect();
    let [_, AgentOutput::Report(Report::ToolServers { servers }), ..] = after[..] else { panic!("{seen:?}") };
    assert_eq!(servers.iter().map(|one| (one.name.as_str(), one.tools)).collect::<Vec<_>>(), [("docs", Some(1))]);
}

/// What it answers is reported without waiting for the next `init`, with the rest of its servers as they were, and the
/// adapter doesn't wait for it, since the CLI can take a while to. Each control once, and each answer once: what each
/// answer makes of either server is `ToolServers`' own tests'.
#[tokio::test]
async fn connecting_to_a_server_again_is_asked_of_the_cli_and_what_it_answers_is_reported() {
    let works = (r#""subtype":"success","request_id":"%s""#, ServerState::Connected);
    let fails = (r#""subtype":"error","request_id":"%s","error":"Sign in to it in Settings.""#, ServerState::Failed);
    let reconnect = Control::Reconnect { server: "linear".into() };
    let reload = Control::Reload { server: "docs".into() };

    for (asks, (answer, came_to)) in [(reconnect, works), (reload, fails)] {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let init = r#"{"type":"system","subtype":"init","session_id":"s","tools":["mcp__docs__find"],"mcp_servers":[{"name":"docs","status":"connected"},{"name":"linear","status":"failed"}]}"#;
        let body = format!(
            r#"printf '%s\n' '{init}'
while read -r line; do case "$line" in *mcp_reconnect*) break;; esac; done
printf '%s' "$line" > asked
id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '{{"type":"control_response","response":{{{answer}}}}}\n' "$id""#
        );
        let agent = spawn_at(script(dir.path(), &body), spec(dir.path()), tx, AgentOptions::default()).await.unwrap();

        let init = servers_said(&mut rx).await;
        agent.control(&asks).await.unwrap();

        let seen = until_exit(&mut rx).await;
        let asked: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("asked")).unwrap()).unwrap();
        let (Control::Reconnect { server } | Control::Reload { server }) = &asks else { panic!("{asks:?}") };
        let expected = serde_json::json!({"subtype": "mcp_reconnect", "serverName": server});
        assert_eq!((&asked["type"], &asked["request"]), (&"control_request".into(), &expected));
        let says = |name: &str, state, tools| ToolServerState {
            name: name.into(),
            hubs: None,
            state,
            error: None,
            tools,
            connects_again: None,
        };
        let listed = vec![says("docs", ServerState::Connected, Some(1)), says("linear", ServerState::Failed, None)];
        // The one asked for has no tools known from then on, and the other is as it was.
        let with_it = |state| {
            let one = |one: &ToolServerState| {
                if one.name == *server { says(server.as_str(), state, None) } else { one.clone() }
            };
            listed.iter().map(one).collect::<Vec<_>>()
        };
        let mut reported = vec![init];
        reported.extend(seen.into_iter().filter_map(servers_of));
        assert_eq!(reported, [listed.clone(), with_it(ServerState::Starting), with_it(came_to)], "{asks:?} {answer}");
    }
}

fn servers_of(said: AgentOutput) -> Option<Vec<ToolServerState>> {
    if let AgentOutput::Report(Report::ToolServers { servers }) = said { Some(servers) } else { None }
}

/// The next the agent says of its servers.
async fn servers_said(output: &mut mpsc::Receiver<AgentOutput>) -> Vec<ToolServerState> {
    let said = async {
        loop {
            if let Some(servers) = servers_of(output.recv().await.expect("output")) {
                return servers;
            }
        }
    };
    timeout(Duration::from_secs(20), said).await.expect("a report")
}
