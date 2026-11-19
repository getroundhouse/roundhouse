#![cfg(test)]
use std::time::Duration;

use protocol::capability::conversation::Start;
use protocol::capability::laws;
use protocol::event::{
    AgentRunState, CompactedBy, CompactionOutcome, ContentBlock, EventBody, ExitReason, SessionEvent, TurnOutcome,
};
use protocol::fixtures;
use protocol::model::{RunId, Seq};
use protocol::prompt::{Prompt, PromptFrom};
use serde_json::{Value, json};

use super::super::state::Ending;
use super::harness::{Harness, msg_chunk, update, user_chunk};
use crate::testing::Asking;
use crate::{AgentOutput, AgentProcess};

const RUN: RunId = RunId::new(1);

fn usage_update(used: u64, size: u64) -> Value {
    update(&json!({"sessionUpdate": "usage_update", "used": used, "size": size}))
}

async fn collect_until_idle(h: &mut Harness) -> (Vec<EventBody>, Vec<Value>, usize) {
    let mut events = Vec::new();
    let mut requests = Vec::new();
    let mut reports = 0;
    loop {
        match h.next().await {
            AgentOutput::Report(_) => reports += 1,
            AgentOutput::Event { body: EventBody::AgentOther { kind, .. }, raw, .. } if kind == "test/request" => {
                if let Some(raw) = raw
                    && let Ok(parsed) = serde_json::from_str::<Value>(&raw)
                {
                    requests.push(parsed["params"].clone());
                }
            }
            AgentOutput::Event { body: EventBody::AgentInit { .. }, .. }
            | AgentOutput::Delta(_)
            | AgentOutput::Retrying(_)
            | AgentOutput::Exited { .. } => {}
            AgentOutput::Event { body, .. } => {
                let idle = matches!(body, EventBody::AgentState { state: AgentRunState::Idle });
                events.push(body);
                if idle {
                    return (events, requests, reports);
                }
            }
        }
    }
}

fn check_laws(asked: Seq, agent_events: &[EventBody]) {
    let mut log =
        vec![SessionEvent { run: None, ..fixtures::event(asked, EventBody::CompactionAsked { keeping: None }) }];
    for (idx, body) in agent_events.iter().enumerate() {
        let seq = Seq::new(asked.get() + 1 + u64::try_from(idx).unwrap());
        log.push(SessionEvent { run: Some(RUN), ..fixtures::event(seq, body.clone()) });
    }
    let exit_seq = Seq::new(asked.get() + 1 + u64::try_from(agent_events.len()).unwrap());
    log.push(SessionEvent {
        run: Some(RUN),
        ..fixtures::event(
            exit_seq,
            EventBody::AgentExited {
                code: Some(0),
                signal: None,
                stderr_tail: Vec::new(),
                reason: ExitReason::Requested,
            },
        )
    });
    let claims = super::super::claims::claims();
    assert_eq!(laws::check(&log, |_| &claims), []);
}

#[tokio::test]
async fn compact_sends_bare_command_suppresses_usage_and_emits_compacted_events_obeying_laws() {
    let script = json!([{
        "on": "session/prompt", "at": 1,
        "actions": [
            usage_update(2560, 1_048_576),
            {"result": {"stopReason": "end_turn", "usage": {"inputTokens": 2500, "outputTokens": 60, "totalTokens": 2560}}}
        ]
    }]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;

    let asked = Seq::new(3);
    agent.compact(asked, None).await.unwrap();
    let (events, requests, reports) = collect_until_idle(&mut h).await;
    agent.stop().await;

    let sent = requests.iter().find(|req| req["method"] == "session/prompt").unwrap();
    assert_eq!(sent["params"]["prompt"], json!([{"type": "text", "text": "/compact"}]));
    assert_eq!(reports, 0, "compaction's own usage_update should not jump the context ring");
    assert_eq!(events[0], EventBody::PromptTaken { prompt: asked });
    assert_eq!(events[1], EventBody::AgentState { state: AgentRunState::Running });
    assert_eq!(events[2], EventBody::AgentCompacting { written_chars: None });
    assert!(
        matches!(
            &events[3],
            EventBody::AgentCompacted {
                asked: Some(seq),
                keeping: None,
                outcome: CompactionOutcome::Compacted {
                    by: CompactedBy::Asked,
                    before_tokens: None,
                    after_tokens: None,
                    took_ms: Some(_),
                    summary: None,
                }
            } if *seq == asked
        ),
        "{events:?}"
    );
    assert!(
        matches!(&events[4], EventBody::TurnEnded { outcome: TurnOutcome::Success, only_compacted: true, .. }),
        "{events:?}"
    );
    check_laws(asked, &events);
}

#[tokio::test]
async fn compact_reports_executor_error_and_rpc_error_as_failed_without_stray_assistant_text() {
    let script = json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                msg_chunk("\n⚠ agent executor error: pre-invocation hook: context summarization: Error 500\n"),
                {"result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                {"error": {"code": -32603, "message": "Internal compaction failure"}}
            ]
        }
    ]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;

    let asked_1 = Seq::new(3);
    agent.compact(asked_1, None).await.unwrap();
    let (events_1, _, _) = collect_until_idle(&mut h).await;

    assert!(!events_1.iter().any(|ev| matches!(ev, EventBody::Assistant { .. })), "{events_1:?}");
    assert!(
        matches!(
            &events_1[3],
            EventBody::AgentCompacted {
                asked: Some(seq),
                keeping: None,
                outcome: CompactionOutcome::Failed { error: Some(msg) }
            } if *seq == asked_1 && msg.contains("context summarization: Error 500")
        ),
        "{events_1:?}"
    );
    assert!(
        matches!(&events_1[4], EventBody::TurnEnded { outcome: TurnOutcome::Success, only_compacted: true, .. }),
        "{events_1:?}"
    );
    check_laws(asked_1, &events_1);

    let asked_2 = Seq::new(20);
    agent.compact(asked_2, None).await.unwrap();
    let (events_2, _, _) = collect_until_idle(&mut h).await;
    agent.stop().await;

    assert!(
        matches!(
            &events_2[3],
            EventBody::AgentCompacted {
                asked: Some(seq),
                keeping: None,
                outcome: CompactionOutcome::Failed { error: Some(msg) }
            } if *seq == asked_2 && msg.contains("Internal compaction failure")
        ),
        "{events_2:?}"
    );
    assert!(
        matches!(&events_2[4], EventBody::TurnEnded { outcome: TurnOutcome::Error, only_compacted: false, .. }),
        "{events_2:?}"
    );
    check_laws(asked_2, &events_2);
}

#[tokio::test]
async fn compact_refuses_keeping_and_mid_turn_and_cancels_cleanly() {
    let script = json!([
        { "on": "session/prompt", "at": 1, "actions": [] },
        {
            "on": "session/cancel", "at": 1,
            "actions": [
                {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}
            ]
        },
        { "on": "session/prompt", "at": 2, "actions": [] },
        {
            "on": "session/cancel", "at": 2,
            "actions": [
                {"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}
            ]
        }
    ]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;

    assert!(agent.compact(Seq::new(2), Some("Keep the API")).await.is_err(), "Keeping::Ignored must reject keeping");

    agent.prompt(Seq::FIRST, &Prompt::new("busy")).await.unwrap();
    assert!(agent.compact(Seq::new(3), None).await.is_err(), "cannot compact mid-turn");
    agent.cancel().await.unwrap();
    let _ = collect_until_idle(&mut h).await;

    let asked = Seq::new(5);
    agent.compact(asked, None).await.unwrap();
    assert!(agent.prompt(Seq::new(6), &Prompt::new("interrupt")).await.is_err(), "cannot prompt while compacting");
    agent.cancel().await.unwrap();
    let (events, _, _) = collect_until_idle(&mut h).await;
    agent.stop().await;

    assert!(!events.iter().any(|ev| matches!(ev, EventBody::AgentCompacted { .. })), "{events:?}");
    assert!(
        matches!(&events[3], EventBody::TurnEnded { outcome: TurnOutcome::Cancelled, only_compacted: false, .. }),
        "{events:?}"
    );
    check_laws(asked, &events);
}

#[tokio::test]
async fn compact_waits_out_recording_flushes_idle_streams_and_ignores_pre_echo_wakeup_text() {
    let script = json!([{
        "on": "session/prompt", "at": 1,
        "actions": [
            msg_chunk("Buffered wakeup prose before /compact echo."),
            user_chunk("/compact"),
            {"result": {"stopReason": "end_turn"}}
        ]
    }]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;
    // Simulate both `Ending::Recording` (right after TurnEnded) and an open idle-streamed text block.
    {
        let mut state = agent.shared.state.lock();
        state.ending = Ending::Recording;
        state.streaming_text = Some(("idle-1".into(), "Idle wakeup text.".into()));
    }
    let watching = agent.shared.turn_done.receiver_count();
    let asked = Seq::new(4);
    let compacting = tokio::spawn({
        let agent = agent.clone();
        async move { agent.compact(asked, None).await }
    });
    waiting::task::until(Duration::from_secs(5), "compact should wait on Ending::Recording", || {
        compacting.is_finished() || agent.shared.turn_done.receiver_count() != watching
    })
    .await;
    assert!(!compacting.is_finished(), "compact must wait for Ending::Recording rather than refusing");
    agent.shared.count_end(&mut agent.shared.state.lock(), Ending::Not);
    tokio::time::timeout(Duration::from_secs(5), compacting).await.unwrap().unwrap().unwrap();

    let (events, _, _) = collect_until_idle(&mut h).await;
    agent.stop().await;

    assert!(
        matches!(
            &events[0],
            EventBody::Assistant { blocks, .. }
                if matches!(blocks.as_slice(), [ContentBlock::Text { text, .. }] if text == "Idle wakeup text.")
        ),
        "idle-streamed text must flush before PromptTaken: {events:?}"
    );
    assert_eq!(events[1], EventBody::PromptTaken { prompt: asked });
    assert!(
        events.iter().any(|ev| matches!(
            ev,
            EventBody::AgentCompacted {
                asked: Some(seq),
                outcome: CompactionOutcome::Compacted { by: CompactedBy::Asked, .. },
                ..
            } if *seq == asked
        )),
        "pre-echo buffered wakeup prose must not mark compaction failed: {events:?}"
    );
}

#[cfg(unix)]
fn push_event(events: &mut Vec<SessionEvent>, next_seq: &mut u64, run: Option<RunId>, body: EventBody) -> Seq {
    let seq = Seq::new(*next_seq);
    *next_seq += 1;
    events.push(SessionEvent { run, ..fixtures::event(seq, body) });
    seq
}

#[cfg(unix)]
async fn record_until(
    rx: &mut tokio::sync::mpsc::Receiver<AgentOutput>,
    events: &mut Vec<SessionEvent>,
    next_seq: &mut u64,
    stop: impl Fn(&EventBody) -> bool,
) {
    loop {
        let body = super::real_cli::recv_event(rx).await;
        let done = stop(&body);
        push_event(events, next_seq, Some(super::real_cli::RUN), body);
        if done {
            return;
        }
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "runs against the installed agy binary"]
async fn real_agy_compact_obeys_laws_and_summarizes_context() {
    use super::real_cli::{RUN, all_text, scripted_gemini, spawn_real_agy, text_sse};

    let room = tempfile::tempdir().unwrap();
    let server = scripted_gemini(|req| {
        if all_text(req).contains("Write a continuation summary") {
            vec![text_sse("<summary>Remembered code ALPHA-99.</summary>")]
        } else {
            vec![text_sse("Noted ALPHA-99.")]
        }
    })
    .await;
    let (agent, mut rx) = spawn_real_agy(room.path(), &server, super::super::claims::full_access()).await;
    let (mut events, mut next_seq) = (Vec::<SessionEvent>::new(), 1_u64);
    let is_idle = |b: &EventBody| matches!(b, EventBody::AgentState { state: AgentRunState::Idle });
    record_until(&mut rx, &mut events, &mut next_seq, |b| matches!(b, EventBody::AgentInit { .. })).await;

    let p1 = push_event(
        &mut events,
        &mut next_seq,
        None,
        EventBody::UserPrompt {
            prompt: Prompt::new("Remember ALPHA-99"),
            client_msg_id: None,
            from: PromptFrom::Person,
        },
    );
    agent.prompt(p1, &Prompt::new("Remember ALPHA-99")).await.unwrap();
    record_until(&mut rx, &mut events, &mut next_seq, is_idle).await;

    let c1 = push_event(&mut events, &mut next_seq, None, EventBody::CompactionAsked { keeping: None });
    agent.compact(c1, None).await.unwrap();
    record_until(&mut rx, &mut events, &mut next_seq, is_idle).await;

    agent.stop().await;
    push_event(
        &mut events,
        &mut next_seq,
        Some(RUN),
        EventBody::AgentExited { code: Some(0), signal: None, stderr_tail: Vec::new(), reason: ExitReason::Requested },
    );
    let claims = super::super::claims::claims();
    assert_eq!(laws::check(&events, |_| &claims), []);
    assert!(events.iter().any(|e| matches!(
        &e.body,
        EventBody::AgentCompacted {
            asked: Some(seq),
            outcome: CompactionOutcome::Compacted { by: CompactedBy::Asked, .. },
            ..
        } if *seq == c1
    )));
    assert!(
        events.iter().any(|e| matches!(
            &e.body,
            EventBody::TurnEnded { outcome: TurnOutcome::Success, only_compacted: true, .. }
        ))
    );
}
