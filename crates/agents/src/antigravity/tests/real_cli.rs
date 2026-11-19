//! End-to-end probes against the real `agy` binary on a scripted loopback Gemini server.
//! Run by hand with: `cargo test -p agents --lib real_cli -- --ignored --nocapture --test-threads=1`

#![cfg(test)]

use crate::testing::Asking;
use std::path::Path;
use std::time::Duration;

use hyper::StatusCode;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Effort;
use protocol::capability::{
    Control,
    approvals::{Asks, Decision, Remember},
    asking::PlanContext,
    conversation::Start,
    laws,
    settings::Settings,
    turns::OwnTurn,
};
use protocol::event::{AgentRunState, EventBody, ExitReason, SessionEvent, TurnOutcome};
use protocol::fixtures;
use protocol::model::{AgentKind, ApprovalId, ModelName, RunId, Seq};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::conformance::http::{self, Server, json_response};
use crate::{AgentOptions, AgentOutput, AgentProcess, BinaryLookup};
use protocol::prompt::PromptFrom;

pub(super) const RUN: RunId = RunId::new(1);

fn find_agy() -> std::path::PathBuf {
    let lookup =
        BinaryLookup { explicit: None, path: std::env::var_os("PATH"), home: std::env::var_os("HOME").map(Into::into) };
    crate::binary::resolve_binary("agy", &[], &lookup).expect("expected `agy` on PATH")
}

pub(super) fn text_sse(text: &str) -> Value {
    json!({
        "candidates": [{ "content": { "role": "model", "parts": [{ "text": text }] }, "finishReason": "STOP" }],
        "usageMetadata": { "promptTokenCount": 80, "candidatesTokenCount": 12, "totalTokenCount": 92 }
    })
}

fn calls_sse(calls: Vec<(&str, Value)>) -> Value {
    let parts: Vec<Value> =
        calls.into_iter().map(|(name, args)| json!({ "functionCall": { "name": name, "args": args } })).collect();
    json!({
        "candidates": [{ "content": { "role": "model", "parts": parts }, "finishReason": "STOP" }],
        "usageMetadata": { "promptTokenCount": 80, "candidatesTokenCount": 16, "totalTokenCount": 96 }
    })
}

fn cmd_call(cmd: &str, summary: &str, wait_ms: u64) -> (&'static str, Value) {
    ("run_command", json!({ "CommandLine": cmd, "toolSummary": summary, "WaitMsBeforeAsync": wait_ms }))
}

pub(super) async fn scripted_gemini(script: impl Fn(&Value) -> Vec<Value> + Send + Sync + 'static) -> Server {
    Server::start(move |path, req, made| {
        if !path.contains(":streamGenerateContent") {
            return json_response(StatusCode::NOT_FOUND, &json!({ "error": "not scripted" }));
        }
        if path.contains("flash-lite") {
            return http::data_events(vec![text_sse("Real agy probe")], made);
        }
        http::data_events(script(req), made)
    })
    .await
    .unwrap()
}

pub(super) async fn spawn_real_agy(
    room: &Path,
    server: &Server,
    mode: ApprovalMode,
) -> (std::sync::Arc<dyn AgentProcess>, mpsc::Receiver<AgentOutput>) {
    let work = room.join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("README.md"), "Real agy probe workspace.\n").unwrap();
    let home = room.join("home");
    let cfg = home.join(".gemini").join("antigravity-cli");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join("settings.json"), json!({ "modelProvider": "gemini" }).to_string()).unwrap();

    let settings = protocol::capability::settings::Settings {
        model: Some(ModelName::named("gemini-3.8-flash")),
        mode: Some(mode),
        effort: Some(Effort::Low),
        ..Default::default()
    };
    let spec = AgentSpec {
        session_id: "real-probe".into(),
        run: RUN,
        agent: AgentKind::Antigravity,
        cwd: work.display().to_string(),
        start: Start::New { conversation: None },
        settings,
        env: vec![
            ("HOME".into(), home.display().to_string()),
            ("GEMINI_API_KEY".into(), "scripted".into()),
            ("GOOGLE_GEMINI_BASE_URL".into(), server.url().into()),
            ("HTTPS_PROXY".into(), "http://127.0.0.1:9".into()),
            ("HTTP_PROXY".into(), "http://127.0.0.1:9".into()),
            ("NO_PROXY".into(), "127.0.0.1,localhost".into()),
        ],
        tools_token: None,
        login: None,
    };
    let (tx, rx) = mpsc::channel(512);
    let agent = super::super::spawn_at(find_agy(), spec, tx, AgentOptions::default()).await.unwrap();
    (agent, rx)
}

pub(super) async fn recv_event(rx: &mut mpsc::Receiver<AgentOutput>) -> EventBody {
    loop {
        let item = tokio::time::timeout(Duration::from_secs(20), rx.recv())
            .await
            .expect("timed out waiting for real agy event")
            .expect("real agy channel closed unexpectedly");
        if let AgentOutput::Event { body, .. } = item {
            return body;
        }
    }
}

pub(super) fn all_text(request: &Value) -> String {
    request["contents"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|c| c["parts"].as_array().into_iter().flatten())
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn last_part_has_fn_response(request: &Value) -> bool {
    request["contents"]
        .as_array()
        .and_then(|c| c.last())
        .and_then(|last| last["parts"].as_array())
        .is_some_and(|parts| parts.iter().any(|p| p.get("functionResponse").is_some()))
}

fn last_user_request(request: &Value) -> String {
    let users: Vec<String> = request["contents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["role"] == "user")
        .map(|c| {
            c["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|t| !t.is_empty())
        .collect();
    users.iter().rev().find(|t| t.contains("<USER_REQUEST>")).or_else(|| users.last()).cloned().unwrap_or_default()
}

fn parallel_bg_script(readme_path: &str, req: &Value) -> Vec<Value> {
    let (full, last_req, has_fn) = (all_text(req), last_user_request(req), last_part_has_fn_response(req));
    if full.contains("beta-99") {
        vec![text_sse("Beta finished too.")]
    } else if !has_fn && last_req.contains("Continue from the completed background task") {
        vec![text_sse("Still waiting on beta.")]
    } else if full.contains("mid-wakeup-44") {
        vec![text_sse("Alpha handled, waiting on beta.")]
    } else if full.contains("alpha-42") {
        vec![calls_sse(vec![
            ("read_file", json!({ "Path": readme_path })),
            cmd_call("echo mid-wakeup-$((40+4))", "Wakeup check", 5000),
        ])]
    } else if has_fn {
        vec![text_sse("Started alpha and beta.")]
    } else {
        vec![calls_sse(vec![
            cmd_call("perl -e 'select undef, undef, undef, 1.2' && echo alpha-$((40+2))", "Alpha task", 500),
            cmd_call("perl -e 'select undef, undef, undef, 2.8' && echo beta-$((90+9))", "Beta task", 500),
        ])]
    }
}

async fn record_until(
    rx: &mut mpsc::Receiver<AgentOutput>,
    record: &mut impl FnMut(Option<RunId>, EventBody) -> Seq,
    done: impl Fn(&EventBody) -> bool,
) {
    loop {
        let body = recv_event(rx).await;
        let stop = done(&body);
        record(Some(RUN), body);
        if stop {
            break;
        }
    }
}

#[tokio::test]
#[ignore = "runs against the installed agy binary"]
async fn real_agy_parallel_background_commands_wake_twice_and_obey_laws() {
    let room = tempfile::tempdir().unwrap();
    let readme_path = room.path().join("work").join("README.md").display().to_string();
    let server = scripted_gemini(move |req| parallel_bg_script(&readme_path, req)).await;
    let (agent, mut rx) = spawn_real_agy(room.path(), &server, super::super::claims::full_access()).await;
    let (mut events, mut next_seq) = (Vec::<SessionEvent>::new(), 1_u64);
    let mut record = |run: Option<RunId>, body: EventBody| {
        let seq = Seq::new(next_seq);
        next_seq += 1;
        events.push(SessionEvent { run, ..fixtures::event(seq, body) });
        seq
    };
    record_until(&mut rx, &mut record, |body| matches!(body, EventBody::AgentInit { .. })).await;
    let p1 = record(
        None,
        EventBody::UserPrompt {
            prompt: Prompt::new("start parallel bg"),
            client_msg_id: None,
            from: PromptFrom::Person,
        },
    );
    agent.prompt(p1, &Prompt::new("start parallel bg")).await.unwrap();

    let (mut turn_results, mut task_ended_turns) = (Vec::new(), 0_usize);
    while turn_results.len() < 3 {
        let body = recv_event(&mut rx).await;
        if matches!(&body, EventBody::OwnTurn(OwnTurn::TaskEnded { .. })) {
            task_ended_turns += 1;
        }
        if let EventBody::TurnEnded { outcome, result, .. } = &body {
            assert_eq!(*outcome, TurnOutcome::Success, "turn failed: {result:?}");
            turn_results.push(result.as_deref().unwrap_or_default().trim().to_owned());
        }
        record(Some(RUN), body);
    }
    record_until(&mut rx, &mut record, |body| matches!(body, EventBody::AgentState { state: AgentRunState::Idle }))
        .await;
    agent.stop().await;
    record(
        Some(RUN),
        EventBody::AgentExited { code: Some(0), signal: None, stderr_tail: Vec::new(), reason: ExitReason::Requested },
    );
    let claims = super::super::claims::claims();
    assert_eq!(laws::check(&events, |_| &claims), []);
    assert_eq!(task_ended_turns, 2);
    assert_eq!(turn_results[0], "Started alpha and beta.");
    assert!(
        matches!(turn_results[1].as_str(), "Alpha handled, waiting on beta." | "Still waiting on beta."),
        "{turn_results:?}"
    );
    assert_eq!(turn_results[2], "Beta finished too.");
}

fn subagent_script(req: &Value) -> Vec<Value> {
    let (full, last_req, has_fn) = (all_text(req), last_user_request(req), last_part_has_fn_response(req));
    if full.contains("Subagent task: run shell") && !full.contains("invoke_subagent") {
        if has_fn {
            vec![text_sse("Child done: child-secret-77")]
        } else {
            vec![calls_sse(vec![cmd_call(
                "perl -e 'select undef, undef, undef, 1.0' && echo child-secret-$((70+7))",
                "Child shell work",
                5000,
            )])]
        }
    } else if !has_fn && last_req.contains("Continue from the completed background task") {
        vec![text_sse("Parent confirmed subagent result: child-secret-77")]
    } else if full.contains("child-secret-77") {
        vec![text_sse("Parent heard: child-secret-77")]
    } else if has_fn {
        vec![text_sse("Delegated to subagent.")]
    } else {
        vec![calls_sse(vec![(
            "invoke_subagent",
            json!({ "TypeName": "self", "Role": "Background Helper", "Prompt": "Subagent task: run shell" }),
        )])]
    }
}

#[tokio::test]
#[ignore = "runs against the installed agy binary"]
async fn real_agy_subagent_runs_command_under_skip_permissions_and_wakes_parent() {
    let room = tempfile::tempdir().unwrap();
    let server = scripted_gemini(subagent_script).await;
    let (agent, mut rx) = spawn_real_agy(room.path(), &server, super::super::claims::full_access()).await;
    while !matches!(recv_event(&mut rx).await, EventBody::AgentInit { .. }) {}
    agent.prompt(Seq::FIRST, &Prompt::new("delegate")).await.unwrap();
    let mut results = Vec::new();
    while results.len() < 2 {
        if let EventBody::TurnEnded { outcome, result, .. } = recv_event(&mut rx).await {
            assert_eq!(outcome, TurnOutcome::Success);
            results.push(result.unwrap_or_default());
        }
    }
    agent.stop().await;
    assert_eq!(results[0], "Delegated to subagent.");
    assert!(results[1].contains("child-secret-77"), "parent wakeup did not surface subagent result: {results:?}");
}

fn mode_switch_plan_script(readme_path: &str, req: &Value) -> Vec<Value> {
    let (full, last_req, has_fn) = (all_text(req), last_user_request(req), last_part_has_fn_response(req));
    if last_req.contains("Implement the approved plan") {
        if full.contains("post-approve-88") {
            vec![text_sse("Executed unasked after approval.")]
        } else {
            vec![calls_sse(vec![cmd_call("echo post-approve-$((80+8))", "Execute approved step", 5000)])]
        }
    } else if full.contains("plan-step2-55") {
        vec![text_sse("1. Run post-approve step")]
    } else if full.contains("plan-bg-33") {
        vec![calls_sse(vec![
            ("read_file", json!({ "Path": readme_path })),
            cmd_call("echo plan-step2-$((50+5))", "Second plan check", 5000),
        ])]
    } else if has_fn {
        vec![text_sse("Waiting for plan research command to finish.")]
    } else {
        vec![calls_sse(vec![cmd_call(
            "perl -e 'select undef, undef, undef, 1.0' && echo plan-bg-$((30+3))",
            "Background plan research",
            500,
        )])]
    }
}

#[tokio::test]
#[ignore = "runs against the installed agy binary"]
async fn real_agy_mode_switch_plan_background_mixed_wakeup_and_resumes_full_access() {
    let room = tempfile::tempdir().unwrap();
    let readme_path = room.path().join("work").join("README.md").display().to_string();
    let server = scripted_gemini(move |req| mode_switch_plan_script(&readme_path, req)).await;
    let (agent, mut rx) = spawn_real_agy(room.path(), &server, super::super::claims::full_access()).await;
    while !matches!(recv_event(&mut rx).await, EventBody::AgentInit { .. }) {}
    let full_pid = agent.pid();

    let to_plan = Settings { mode: Some(super::super::claims::plan()), ..Settings::default() };
    agent.control(&Control::Change(to_plan)).await.unwrap();
    let plan_pid = agent.pid();
    assert_ne!(plan_pid, full_pid, "switching FullAccess -> Plan must restart agy without skip-permissions");

    agent.prompt(Seq::FIRST, &Prompt::new("draft plan")).await.unwrap();
    let call1 = wait_call_approval(&mut rx).await;
    let allow = Decision::Allow { remember: Remember::Once };
    agent.control(&Control::Decide { approval: call1, decision: allow.clone() }).await.unwrap();

    let status_turn = loop {
        let ev = recv_event(&mut rx).await;
        assert!(!matches!(ev, EventBody::ApprovalRequested { .. }), "status prose held prematurely: {ev:?}");
        if let EventBody::TurnEnded { result, .. } = ev {
            break result.unwrap_or_default();
        }
    };
    assert_eq!(status_turn, "Waiting for plan research command to finish.");

    let call2 = wait_call_approval(&mut rx).await;
    agent.control(&Control::Decide { approval: call2, decision: allow }).await.unwrap();

    let plan_approval: ApprovalId = loop {
        if let EventBody::ApprovalRequested { approval_id, request: req, .. } = recv_event(&mut rx).await
            && matches!(&req.asks, Asks::Plan { plan, .. } if plan == "1. Run post-approve step")
        {
            break approval_id;
        }
    };
    let approve = Decision::ApprovePlan { context: PlanContext::Kept };
    agent.control(&Control::Decide { approval: plan_approval, decision: approve }).await.unwrap();
    assert_ne!(agent.pid(), plan_pid, "ApprovePlan must restart agy WITH --dangerously-skip-permissions");

    let final_result = loop {
        let ev = recv_event(&mut rx).await;
        assert!(!matches!(ev, EventBody::ApprovalRequested { .. }), "FullAccess must not ask permission: {ev:?}");
        if let EventBody::TurnEnded { outcome, result, .. } = ev {
            assert_eq!(outcome, TurnOutcome::Success);
            break result.unwrap_or_default();
        }
    };
    assert_eq!(final_result, "Executed unasked after approval.");
    agent.stop().await;
}

async fn wait_call_approval(rx: &mut mpsc::Receiver<AgentOutput>) -> ApprovalId {
    loop {
        if let EventBody::ApprovalRequested { approval_id, request: req, .. } = recv_event(rx).await
            && matches!(req.asks, Asks::Call { .. })
        {
            return approval_id;
        }
    }
}
