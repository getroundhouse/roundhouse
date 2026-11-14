//! A held login is pinned and checked in the process that receives the prompt, without leaking it to captures.

use agents::conformance::capture::Tap;
use agents::{AgentOptions, AgentOutput, BinaryLookup};
use protocol::model::{AgentKind, AgentLogin, Effort, LoginToken, RunId, Seq, SessionSettingsChange};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

static PROGRAM: agents::testing::Example = agents::testing::Example::new("login_standin");

const TOKEN: &str = "sentinel-selected-token";

fn spec(dir: &Path) -> AgentSpec {
    AgentSpec {
        session_id: "s".into(),
        run: RunId::new(1),
        agent: AgentKind::Claude,
        cwd: dir.to_string_lossy().into_owned(),
        start: protocol::capability::conversation::Start::New { conversation: Some("s".into()) },
        settings: {
            let description = agents::claude::description();
            description.defaults.for_capabilities(&description.capabilities.unwrap()).unwrap()
        },
        env: vec![],
        tools_token: None,
        login: None,
    }
}

/// The credential settings observed in a real `get_settings` reply, with a synthetic token.
fn settings() -> Value {
    json!({"effective":{"apiKeyHelper":"", "env":{
        "CLAUDE_CODE_USE_BEDROCK":"", "CLAUDE_CODE_USE_VERTEX":"", "CLAUDE_CODE_USE_FOUNDRY":"",
        "CLAUDE_CODE_USE_MANTLE":"", "CLAUDE_CODE_USE_ANTHROPIC_AWS":"", "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD":"",
        "ANTHROPIC_AUTH_TOKEN":"", "ANTHROPIC_API_KEY":"", "ANTHROPIC_CUSTOM_HEADERS":"",
        "ANTHROPIC_BASE_URL":"https://api.anthropic.com", "CLAUDE_CODE_OAUTH_TOKEN":TOKEN}},
        "applied":{"model":"opus", "effort":"high"}, "sources":{"user":"sentinel-private-settings"}})
}

/// Records stdin in this synthetic test only. Replies carry the real control response envelope.
fn options(dir: &Path, settings: &Value) -> AgentOptions {
    let program = PROGRAM.path();
    let bin = dir.join(format!("login-cli{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(program, &bin).unwrap();
    std::fs::write(dir.join("login-script.json"), json!({"settings":settings}).to_string()).unwrap();
    AgentOptions {
        binary: BinaryLookup { explicit: Some(bin), ..BinaryLookup::default() },
        login: Some(AgentLogin { token: LoginToken::new(TOKEN) }),
        ..AgentOptions::default()
    }
}

#[tokio::test]
async fn a_prompt_and_a_later_effort_read_use_the_checked_login_without_capturing_its_token() {
    let dir = tempfile::tempdir().unwrap();
    let options = options(dir.path(), &settings());
    let (output, mut received) = mpsc::channel(32);
    let (tap, mut capture) = Tap::new();
    let agent =
        agents::claude::spawn_tapped(options.binary.explicit.clone().unwrap(), spec(dir.path()), output, options, tap)
            .await
            .unwrap();
    agent
        .configure(&SessionSettingsChange { effort: Some(Effort::High), ..SessionSettingsChange::default() })
        .await
        .unwrap();
    agent.send_prompt(Seq::new(1), &Prompt::new("hello")).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(item) = received.recv().await {
            if matches!(item, AgentOutput::Event { body: protocol::event::EventBody::AgentInit { .. }, .. }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    agent.stop().await;
    let entries: Vec<_> = std::iter::from_fn(|| capture.try_recv().ok()).collect();
    let captured = format!("{entries:?}");
    assert!(!captured.contains("sentinel-"), "{captured}");
    let input = std::fs::read_to_string(dir.path().join("received")).unwrap();
    let lines: Vec<Value> = input.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(lines[0]["request"]["subtype"], "apply_flag_settings");
    assert_eq!(lines[0]["request"]["settings"]["env"]["CLAUDE_CODE_OAUTH_TOKEN"], TOKEN);
    assert_eq!(lines[1]["request"]["subtype"], "initialize");
    assert_eq!(lines[2]["request"]["subtype"], "get_settings");
    assert!(lines.iter().any(|line| line["type"] == "user"));
}

#[tokio::test]
async fn unconfirmed_settings_refuse_session_and_title_before_either_receives_a_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let options = options(dir.path(), &json!({"effective":{}}));
    let (output, _) = mpsc::channel(32);
    let error = agents::claude::spawn(spec(dir.path()), output, options.clone()).await.err().expect("refused");
    assert!(!error.to_string().contains(TOKEN));
    let input = std::fs::read_to_string(dir.path().join("received")).unwrap();
    assert!(!input.contains("\"type\":\"user\""));
    let title = agents::claude::title_for(&Prompt::new("hello"), &options, dir.path()).await;
    assert!(title.is_err());
    let input = std::fs::read_to_string(dir.path().join("received")).unwrap();
    assert!(!input.contains("\"type\":\"user\""));
}

#[tokio::test]
async fn titles_use_the_same_checked_login() {
    let dir = tempfile::tempdir().unwrap();
    let options = options(dir.path(), &settings());
    let title = agents::claude::title_for(&Prompt::new("hello"), &options, dir.path()).await.unwrap();
    assert_eq!(title.as_str(), "Checked login");
}

fn mode(dir: &Path, mode: &str) {
    std::fs::write(dir.join("login-script.json"), json!({"settings":settings(), "mode":mode}).to_string()).unwrap();
}

/// Wait for the stand-in to reach a particular control request, not for an assumed startup duration.
async fn ready(path: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_corrected_launch_is_checked_again_and_startup_output_survives_the_check() {
    for scenario in ["retry-before", "startup-output"] {
        let dir = tempfile::tempdir().unwrap();
        let options = options(dir.path(), &settings());
        mode(dir.path(), scenario);
        let (output, mut received) = mpsc::channel(32);
        let agent = agents::claude::spawn(spec(dir.path()), output, options).await.unwrap();
        agent.send_prompt(Seq::new(1), &Prompt::new("hello")).await.unwrap();
        let mut seen = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(item) = received.recv().await {
                let initialized =
                    matches!(item, AgentOutput::Event { body: protocol::event::EventBody::AgentInit { .. }, .. });
                seen.push(item);
                if initialized {
                    break;
                }
            }
        })
        .await
        .unwrap();
        agent.stop().await;
        let attempts = std::fs::read_to_string(dir.path().join("attempts")).unwrap();
        assert_eq!(attempts, if scenario == "retry-before" { "2" } else { "1" });
        if scenario == "startup-output" {
            assert!(format!("{seen:?}").contains("buffered-startup"));
        }
    }
}

#[tokio::test]
async fn stopping_during_replacement_verification_cancels_it_without_replaying_the_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let options = options(dir.path(), &settings());
    mode(dir.path(), "retry-stop");
    let (output, _received) = mpsc::channel(32);
    let agent = agents::claude::spawn(spec(dir.path()), output, options).await.unwrap();
    agent.send_prompt(Seq::new(1), &Prompt::new("hello")).await.unwrap();
    ready(&dir.path().join("guard-ready")).await;
    tokio::time::timeout(Duration::from_secs(5), agent.stop()).await.expect("stop reaches the unverified replacement");
    let input = std::fs::read_to_string(dir.path().join("received")).unwrap();
    let users = input.lines().filter(|line| serde_json::from_str::<Value>(line).unwrap()["type"] == "user").count();
    assert_eq!(users, 1, "nothing replays into the stopped replacement");
}

#[tokio::test]
async fn cancelling_startup_stops_the_unverified_process_and_removes_its_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut options = options(dir.path(), &settings());
    let record = dir.path().join("pid");
    options.pid_file = Some(record.clone());
    mode(dir.path(), "hold-start");
    let (output, _received) = mpsc::channel(32);
    let spec = spec(dir.path());
    let starting = tokio::spawn(async move { agents::claude::spawn(spec, output, options).await });
    ready(&dir.path().join("guard-ready")).await;
    let process = process::ProcessRecord::read(&record).unwrap();
    starting.abort();
    assert!(starting.await.err().unwrap().is_cancelled());
    assert!(!record.exists());
    tokio::time::timeout(Duration::from_secs(5), async {
        while process.is_running() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the cancelled process is reaped");
}
