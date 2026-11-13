use protocol::event::{EventBody, ExitReason};
use protocol::model::RunId;
use protocol::tail::{DeltaChange, StreamDelta};

use super::*;

fn limits() -> WorkerFrame {
    WorkerFrame::AgentLimits {
        session_id: "s".into(),
        run: RunId::new(1),
        limits: Vec::new(),
        every_models_week: false,
        seq: None,
    }
}

fn delta() -> WorkerFrame {
    let change = DeltaChange::Text { index: 0, text: "hi".into() };
    let delta = StreamDelta { run: None, message_id: "m".into(), parent_tool_use_id: None, change };
    WorkerFrame::AgentDelta { session_id: "s".into(), run: RunId::new(1), delta }
}

/// Thinking that the agent of `run` writes in its message, `m` and the run.
fn thinking(run: u64, text: &str) -> WorkerFrame {
    let change = DeltaChange::Thinking { index: 0, text: text.into() };
    let delta = StreamDelta { run: None, message_id: format!("m{run}").into(), parent_tool_use_id: None, change };
    WorkerFrame::AgentDelta { session_id: "s".into(), run: RunId::new(run), delta }
}

/// The agent of run 1 finishing its message.
fn finished() -> WorkerFrame {
    let body = EventBody::Assistant {
        message_id: Some("m1".into()),
        parent_tool_use_id: None,
        model: None,
        blocks: Vec::new(),
        first_index: None,
    };
    WorkerFrame::AgentEvent {
        session_id: "s".into(),
        run: RunId::new(1),
        body,
        raw: None,
        attachments: Vec::new(),
        seq: None,
    }
}

/// The agent of `run` exiting.
fn exited(run: u64) -> WorkerFrame {
    let body = EventBody::AgentExited { reason: ExitReason::Requested, code: None, signal: None, stderr_tail: vec![] };
    WorkerFrame::AgentEvent {
        session_id: "s".into(),
        run: RunId::new(run),
        body,
        raw: None,
        attachments: Vec::new(),
        seq: None,
    }
}

/// The runs each frame says something is being written by, and what, in the order they were sent.
fn being_written(frames: &[Arc<WorkerFrame>]) -> Vec<(u64, String)> {
    let written = |frame: &Arc<WorkerFrame>| {
        let WorkerFrame::AgentTail { run, tail, .. } = frame.as_ref() else { return None };
        let text = tail.messages.iter().flat_map(|m| &m.blocks).map(|b| b.text.as_str()).collect();
        Some((run.get(), text))
    };
    frames.iter().filter_map(written).collect()
}

fn number(frame: &WorkerFrame) -> Option<u64> {
    if let WorkerFrame::AgentLimits { seq, .. } = frame { *seq } else { None }
}

/// What a hub on a channel of its own has been sent so far.
fn sent(hub: &mut mpsc::Receiver<Arc<WorkerFrame>>) -> Vec<Arc<WorkerFrame>> {
    std::iter::from_fn(|| hub.try_recv().ok()).collect()
}

#[tokio::test]
async fn only_for_a_hub_that_asked_is_what_it_records_numbered_and_kept() {
    let outbox = Outbox::default();
    let (first, mut first_hub) = mpsc::channel(8);
    outbox.attach(first, false).await;
    outbox.send(limits()).await;
    let (second, mut second_hub) = mpsc::channel(8);
    outbox.attach(second, true).await;
    outbox.send(limits()).await;
    outbox.send(delta()).await;
    outbox.send(limits()).await;

    assert_eq!(sent(&mut first_hub).iter().map(|f| number(f)).collect::<Vec<_>>(), [None]);
    let numbers: Vec<_> = sent(&mut second_hub).iter().map(|f| number(f)).collect();
    assert_eq!(numbers, [Some(1), None, Some(2)], "numbered from the first hub that asked, but for the delta");
    let (third, mut third_hub) = mpsc::channel(8);
    outbox.attach(third, true).await;
    let numbers: Vec<_> = sent(&mut third_hub).iter().map(|f| number(f)).collect();
    assert_eq!(numbers, [Some(1), Some(2), None], "and what the delta wrote, which nothing has finished");
}

#[tokio::test]
async fn the_next_hub_gets_what_the_last_didnt_record_before_anything_new() {
    let outbox = Outbox::default();
    let (first, _gone) = mpsc::channel(8);
    outbox.attach(first, true).await;
    for _ in 0..3 {
        outbox.send(limits()).await;
    }
    outbox.received(1).await;
    outbox.detach().await;
    outbox.send(limits()).await;

    let (next, mut next_hub) = mpsc::channel(8);
    outbox.attach(next, true).await;
    outbox.send(limits()).await;

    let numbers: Vec<_> = sent(&mut next_hub).iter().map(|f| number(f)).collect();
    assert_eq!(numbers, [Some(2), Some(3), Some(4), Some(5)]);
}

#[tokio::test]
async fn a_frame_sent_with_no_hub_connected_waits_for_the_next_one() {
    let outbox = Outbox::default();
    let (gone, gone_hub) = mpsc::channel(8);
    outbox.attach(gone, true).await;
    drop(gone_hub);
    outbox.send(limits()).await;
    outbox.send(limits()).await;

    let (next, mut next_hub) = mpsc::channel(8);
    outbox.attach(next, true).await;

    assert_eq!(sent(&mut next_hub).len(), 2, "the hub that went missed both");
}

#[tokio::test]
async fn the_next_hub_gets_what_an_agent_is_writing_after_what_was_kept_and_before_anything_new() {
    let outbox = Outbox::default();
    let (first, _gone) = mpsc::channel(8);
    outbox.attach(first, true).await;
    outbox.send(thinking(1, "Reading ")).await;
    outbox.send(limits()).await;
    outbox.detach().await;
    outbox.send(thinking(1, "the code")).await;

    let (next, mut next_hub) = mpsc::channel(8);
    outbox.attach(next, true).await;
    outbox.send(thinking(1, ", then")).await;

    let sent = sent(&mut next_hub);
    assert_eq!(sent.iter().map(|f| number(f)).collect::<Vec<_>>(), [Some(1), None, None], "{sent:#?}");
    assert_eq!(being_written(&sent), [(1, "Reading the code".to_string())], "with what was written with no hub there");
    assert!(matches!(sent[2].as_ref(), WorkerFrame::AgentDelta { .. }), "{sent:#?}");
}

#[tokio::test]
async fn a_hub_hears_of_each_run_still_writing_and_of_no_message_that_s_finished() {
    let outbox = Outbox::default();
    outbox.send(thinking(1, "done")).await;
    outbox.send(finished()).await;
    let (first, mut first_hub) = mpsc::channel(8);
    outbox.attach(first, true).await;
    outbox.send(thinking(1, "old")).await;
    outbox.send(thinking(2, "new")).await;

    let (next, mut next_hub) = mpsc::channel(8);
    outbox.attach(next, true).await;

    assert!(being_written(&sent(&mut first_hub)).is_empty(), "the finished message is the event's now");
    let written = being_written(&sent(&mut next_hub));
    assert_eq!(written, [(1, "old".to_string()), (2, "new".to_string())], "a replaced run's message isn't over yet");
}

/// What a worker keeps of a session goes once nothing is being written in it, as its message finishes or its agent
/// exits, so it lasts no longer than the run.
#[tokio::test]
async fn nothing_is_kept_of_a_session_once_its_message_finishes_or_its_agent_exits() {
    let outbox = Outbox::default();
    let kept_of_any = async || !outbox.0.lock().await.writing.0.is_empty();

    outbox.send(thinking(1, "done")).await;
    outbox.send(finished()).await;
    let after_finishing = kept_of_any().await;
    outbox.send(thinking(1, "cut off")).await;
    let while_writing = kept_of_any().await;
    outbox.send(exited(1)).await;

    assert!(!after_finishing);
    assert!(while_writing);
    assert!(!kept_of_any().await, "the agent took what it was writing with it");
}
