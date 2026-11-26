#![cfg(test)]
use super::harness::Harness;
use crate::{AgentOutput, AgentProcess, HubToolServer, ToolServers};
use protocol::worker::ToolsToken;
use serde_json::json;

#[tokio::test]
async fn abandoning_initialization_kills_the_process_group_and_removes_its_record() {
    let mut h = Harness::new(&json!([{"on":"initialize","actions":[
        "descendant",{"method":"test/ready","params":{}}
    ]}]));
    loop {
        if let AgentOutput::Event { body: protocol::event::EventBody::AgentOther { kind, .. }, .. } = h.next().await
            && kind == "test/ready"
        {
            break;
        }
    }
    let pid_path = h.room.path().join("pid");
    let pid = process::ProcessRecord::read(&pid_path).unwrap();
    let child: u32 = std::fs::read_to_string(h.room.path().join("descendant")).unwrap().trim().parse().unwrap();
    assert!(pid.is_running());
    assert!(process::start_time(child).is_some());
    h.launch.abort();
    let _ = (&mut h.launch).await;
    loop {
        if matches!(h.next().await, AgentOutput::Exited { .. }) {
            break;
        }
    }
    assert!(!pid.is_running());
    // Killed with its group, the descendant still has its PID until something reaps it, so wait for it to go.
    waiting::task::until(std::time::Duration::from_secs(5), "the descendant outlived its process group", || {
        !process::testing::is_running(child)
    })
    .await;
    assert!(!pid_path.exists());
}

#[tokio::test]
async fn a_thread_is_started_with_each_tool_server_under_a_key_of_its_own() {
    let servers = ToolServers {
        tools_url: "http://127.0.0.1:50123/mcp".into(),
        hubs: vec![HubToolServer {
            id: "t-1".into(),
            name: "linear".into(),
            url: "http://127.0.0.1:50123/tool-servers/t-1".into(),
        }],
        token: ToolsToken::new("token-s"),
        config: "/no/file".into(),
    };
    let mut h = Harness::with_tools(&json!([]), Some(servers));

    let agent = h.started().await;
    let started = h.request("thread/start").await;

    let headers = json!({ "Authorization": "Bearer token-s" });
    let config = &started["params"]["config"];
    let tools = json!({ "url": "http://127.0.0.1:50123/mcp", "http_headers": headers });
    assert_eq!(config["mcp_servers.roundhouse"], tools);
    let linear = json!({ "url": "http://127.0.0.1:50123/tool-servers/t-1", "http_headers": headers });
    assert_eq!(config["mcp_servers.linear"], linear);
    agent.stop().await;
}

#[tokio::test]
async fn cancelling_an_account_call_stops_its_native_process() {
    use crate::{AgentOptions, BinaryLookup};
    let mut h = Harness::new(&json!([{"on":"account/read","actions":["account-ready"]}]));
    h.started().await.stop().await;
    let room = h.room.path().to_owned();
    let pid_path = room.join("account-pid");
    let options = AgentOptions {
        binary: BinaryLookup { explicit: Some(super::harness::PROGRAM.path()), ..Default::default() },
        pid_file: Some(pid_path.clone()),
        ..Default::default()
    };
    let call_room = room.clone();
    let call = tokio::spawn(async move { crate::codex::account(&options, &call_room).await });
    let ready = room.join("account-ready");
    waiting::task::until(std::time::Duration::from_secs(10), "the account call never started Codex", || ready.exists())
        .await;
    let record = process::ProcessRecord::read(&pid_path).unwrap();
    assert!(record.is_running());
    call.abort();
    let _ = call.await;
    let failure = "cancelling the call never took its process's record away";
    waiting::task::until(std::time::Duration::from_secs(10), failure, || !pid_path.exists()).await;
    assert!(!record.is_running());
}
