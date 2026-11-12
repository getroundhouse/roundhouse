use protocol::model::RunId;
use protocol::tail::{DeltaChange, StreamDelta};

use super::*;

fn limits() -> WorkerFrame {
    WorkerFrame::AgentLimits { session_id: "s".into(), run: RunId::new(1), limits: Vec::new(), seq: None }
}

fn delta() -> WorkerFrame {
    let change = DeltaChange::Text { index: 0, text: "hi".into() };
    let delta = StreamDelta { run: None, message_id: "m".into(), parent_tool_use_id: None, change };
    WorkerFrame::AgentDelta { session_id: "s".into(), run: RunId::new(1), delta }
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
    assert_eq!(sent(&mut third_hub).iter().map(|f| number(f)).collect::<Vec<_>>(), [Some(1), Some(2)]);
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
