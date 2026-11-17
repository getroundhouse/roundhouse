#![cfg(test)]
use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::capability::conversation::Start;
use protocol::model::PermissionMode;
use protocol::prompt::Prompt;
use protocol::worker::ToolsToken;
use serde_json::{Value, json};

use super::harness::{Harness, PROGRAM};
use crate::{AgentOptions, AgentProcess, BinaryLookup, ToolServer};

#[tokio::test]
async fn title_for_returns_title_in_plan_mode_without_asking_and_cleans_up_session_files() {
    let room = tempfile::tempdir().unwrap();
    let home = room.path().join("home");
    let cli_root = seed_title_session_files(&home, "title-sess");
    let script = json!([
        {"on": "session/new", "actions": [{"result": {"sessionId": "title-sess", "configOptions": []}}]},
        {
            "on": "session/prompt",
            "actions": [
                {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "title-sess", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "**Title: Fix login redirect.**\n"}}}},
                {"result": {"stopReason": "end_turn"}}
            ]
        }
    ]);
    std::fs::write(room.path().join("native-script.json"), serde_json::to_vec(&script).unwrap()).unwrap();

    let options = AgentOptions {
        binary: BinaryLookup { explicit: Some(PROGRAM.path()), home: Some(home), ..Default::default() },
        tools: Some(ToolServer {
            url: "http://127.0.0.1:9/mcp".into(),
            token: ToolsToken::new("secret-token"),
            config: room.path().join("mcp.json"),
        }),
        ..Default::default()
    };
    let title = crate::antigravity::title_for(&Prompt::new("Fix the broken login redirect"), &options, room.path())
        .await
        .unwrap();
    assert_eq!(title.as_str(), "Fix login redirect");

    let recorded: Vec<Value> = std::fs::read_to_string(room.path().join("native-requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        recorded[0]["launch_args"],
        json!(["acp", "--disable-telemetry", "--include-default-customizations=false"])
    );
    let session_new = recorded.iter().find(|entry| entry["method"] == "session/new").expect("session/new");
    assert_eq!(session_new["params"]["mcpServers"], json!([]));
    assert!(
        session_new["params"]["_meta"]["antigravity"]["systemPrompt"]["append"]
            .as_str()
            .is_some_and(|s| s.contains("You name tasks"))
    );
    let session_close = recorded.iter().find(|entry| entry["method"] == "session/close").expect("session/close");
    assert_eq!(session_close["params"]["sessionId"], "title-sess");

    assert!(!cli_root.join("conversations").join("title-sess.db").exists());
    assert!(!cli_root.join("annotations").join("title-sess.pbtxt").exists());
    assert!(!cli_root.join("brain").join("title-sess").exists());
}

fn seed_title_session_files(home: &Path, id: &str) -> PathBuf {
    let cli_root = home.join(".gemini").join("antigravity-cli");
    std::fs::create_dir_all(cli_root.join("conversations")).unwrap();
    std::fs::create_dir_all(cli_root.join("annotations")).unwrap();
    std::fs::create_dir_all(cli_root.join("brain").join(id)).unwrap();
    std::fs::write(cli_root.join("conversations").join(format!("{id}.db")), b"db").unwrap();
    std::fs::write(cli_root.join("annotations").join(format!("{id}.pbtxt")), b"ann").unwrap();
    std::fs::write(cli_root.join("brain").join(id).join("note.md"), b"n").unwrap();
    cli_root
}

#[tokio::test]
async fn agent_launch_passes_roundhouse_mcp_server_in_session_new_with_bearer_header() {
    let room = tempfile::tempdir().unwrap();
    let tools = ToolServer {
        url: "http://127.0.0.1:50123/mcp".into(),
        token: ToolsToken::new("tok-123"),
        config: room.path().join("mcp.json"),
    };
    let mut h =
        Harness::with_tools(&json!([]), PermissionMode::FullAccess, Start::New { conversation: None }, Some(tools));
    let agent = h.started().await;
    let session_new = h.request("session/new").await;
    assert_eq!(
        session_new["params"]["mcpServers"],
        json!([{
            "type": "http",
            "name": "roundhouse",
            "url": "http://127.0.0.1:50123/mcp",
            "headers": [{"name": "Authorization", "value": "Bearer tok-123"}]
        }])
    );
    agent.stop().await;
}

#[tokio::test]
async fn account_reads_models_and_cancelling_kills_its_process_group() {
    let room = tempfile::tempdir().unwrap();
    std::fs::write(
        room.path().join("native-models.json"),
        serde_json::to_vec(&json!({"stdout": "gemini-4-carbon\nclaude-sonnet-4-6\n", "exit": 0})).unwrap(),
    )
    .unwrap();
    let options = AgentOptions {
        binary: BinaryLookup { explicit: Some(PROGRAM.path()), ..Default::default() },
        ..Default::default()
    };
    let account = crate::antigravity::account(&options, room.path()).await.unwrap();
    assert!(account.logged_in);
    assert_eq!(account.login.as_deref(), Some("oauth"));

    std::fs::write(
        room.path().join("native-models.json"),
        serde_json::to_vec(&json!({"hold": true, "descendant": true})).unwrap(),
    )
    .unwrap();
    let pid_path = room.path().join("account-pid");
    let hold_options = AgentOptions {
        binary: BinaryLookup { explicit: Some(PROGRAM.path()), ..Default::default() },
        pid_file: Some(pid_path.clone()),
        ..Default::default()
    };
    let call_room = room.path().to_owned();
    let call = tokio::spawn(async move { crate::antigravity::account(&hold_options, &call_room).await });
    waiting::task::until(Duration::from_secs(10), "the models to be ready", || {
        room.path().join("models-ready").exists()
    })
    .await;
    let record = process::ProcessRecord::read(&pid_path).unwrap();
    let child: u32 = std::fs::read_to_string(room.path().join("descendant")).unwrap().trim().parse().unwrap();
    assert!(record.is_running());
    assert!(process::start_time(child).is_some());
    call.abort();
    let _ = call.await;
    waiting::task::until(Duration::from_secs(10), "the account call's process and its descendant to go", || {
        !pid_path.exists() && !record.is_running() && process::start_time(child).is_none()
    })
    .await;
}
