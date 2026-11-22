//! What the agent is doing, said for it while it can't answer what was said out loud, as its adapter reports it, and
//! how long every subscriber's tail has it.

use std::sync::Arc;

use protocol::capability::output::{CoverLine, CoverWait};
use protocol::capability::{Capabilities, Claimed, Report};
use protocol::event::ExitReason;
use protocol::model::{ClientDevice, RunCapabilities, RunId};
use protocol::tail::Tail;
use tokio::sync::broadcast;

use super::super::{Item, Live, LiveItem, LiveSession};
use crate::hub::Hub;
use crate::hub::tests::{carried_on, flushed, hub_in, taking_worker};
use crate::worker_link::WorkerLinkId;

const RUN: RunId = RunId::new(2);

fn device(id: &str) -> ClientDevice {
    ClientDevice { id: id.into(), name: format!("Mac {id}") }
}

fn line(wait: u32, number: u32) -> CoverLine {
    CoverLine { wait: CoverWait(wait), number, text: "Hold on.".into() }
}

/// The session `s`, whose agent of [`RUN`] claims to say lines, with its hub and the worker its agent is on.
fn claiming_cover_lines(dir: &std::path::Path) -> (Arc<Hub>, Arc<LiveSession>, WorkerLinkId) {
    let hub = hub_in(dir);
    let (link, _) = taking_worker(&hub);
    let live = carried_on(&hub, &link, RUN);
    let mut claims = Capabilities::default();
    claims.output.cover_lines = Some(Claimed {});
    live.state.lock().capabilities = Some(RunCapabilities::new(RUN, claims));
    (hub, live, link.id)
}

/// The run `by` reports `report`, and the line the tail has once the session has dealt with it.
async fn reported(live: &LiveSession, by: RunId, report: Report) -> Option<(Option<RunId>, CoverLine)> {
    live.send_item(Item::Report { run: by, report, seq: None }).await;
    said(live).await
}

async fn said(live: &LiveSession) -> Option<(Option<RunId>, CoverLine)> {
    flushed(live).await;
    live.snapshot().tail.covering.map(|covering| (covering.run, covering.line))
}

/// How many times the session's followers have been told to read its tail again since `following` was last asked.
fn told_to_read_again(following: &mut broadcast::Receiver<Live>) -> usize {
    std::iter::from_fn(|| following.try_recv().ok()).filter(|live| matches!(live.item, LiveItem::Rewritten)).count()
}

/// A line reaches every subscriber's tail only from the run that's going while conversation mode is on, since only
/// then is anyone listening for it, and only its wait closing ends it.
#[tokio::test]
async fn a_line_said_for_the_agent_shows_from_its_run_while_the_mode_is_on_until_its_wait_closes() {
    let dir = tempfile::tempdir().unwrap();
    let (_hub, live, _) = claiming_cover_lines(dir.path());
    let says = |wait, number| Report::Covering(line(wait, number));
    let mut following = live.subscribe();

    assert_eq!(reported(&live, RUN, says(1, 1)).await, None, "conversation mode is off");
    live.start_conversation(device("d1")).await.unwrap();
    told_to_read_again(&mut following);
    assert_eq!(reported(&live, RunId::new(1), says(1, 1)).await, None, "a replaced run's");
    assert_eq!(told_to_read_again(&mut following), 0);

    assert_eq!(reported(&live, RUN, says(1, 1)).await, Some((Some(RUN), line(1, 1))));
    assert_eq!(told_to_read_again(&mut following), 1);
    assert_eq!(reported(&live, RUN, says(1, 2)).await, Some((Some(RUN), line(1, 2))));
    let closed = |wait| Report::Covered { wait: CoverWait(wait) };
    assert_eq!(reported(&live, RUN, closed(0)).await, Some((Some(RUN), line(1, 2))), "an earlier wait's closing");
    told_to_read_again(&mut following);
    assert_eq!(reported(&live, RUN, closed(1)).await, None);
    assert_eq!(told_to_read_again(&mut following), 1);

    live.state.lock().capabilities = Some(RunCapabilities::new(RUN, Capabilities::default()));
    assert_eq!(reported(&live, RUN, says(2, 1)).await, None, "a run that doesn't claim them, as Codex's doesn't");
}

/// A hub that connects to the worker again takes the line from what the worker kept of the run, as it does what the
/// agent is writing, unless the mode is off, and a line goes with its agent, since nothing will close its wait.
#[tokio::test]
async fn a_line_handed_over_by_the_worker_shows_only_while_the_mode_is_on_and_goes_with_its_agent() {
    let dir = tempfile::tempdir().unwrap();
    let (_hub, live, link) = claiming_cover_lines(dir.path());
    let mut kept = Tail::default();
    kept.covering(RUN, line(3, 1));
    let handed_over = async |tail: &Tail| {
        live.send_item(Item::Tail { run: RUN, tail: tail.clone() }).await;
        said(&live).await
    };

    assert_eq!(handed_over(&kept).await, None, "conversation mode is off");
    live.start_conversation(device("d1")).await.unwrap();
    assert_eq!(handed_over(&kept).await, Some((Some(RUN), line(3, 1))));
    assert_eq!(handed_over(&Tail::default()).await, None, "the worker's wait had closed");

    handed_over(&kept).await;
    live.send_item(Item::AgentLost { link, reason: ExitReason::Requested }).await;
    assert_eq!(said(&live).await, None);
}
