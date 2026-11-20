//! What the agent is doing, said for it while it can't answer a prompt said out loud, as its adapter reports it.

use protocol::capability::output::Covering;
use protocol::capability::{Capabilities, Claimed, Report};
use protocol::model::{ClientDevice, RunCapabilities, RunId, Seq};

use super::super::Item;
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};

/// The line reaches every subscriber's tail only from the run that's going, while conversation mode is on, since only
/// then is anyone listening for it, and from a run that claims to say them.
#[tokio::test]
async fn a_line_said_for_the_agent_reaches_the_tail_only_from_its_run_while_the_mode_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let run = RunId::new(2);
    let live = carried_on(&hub, &stand_in_worker(&hub), run);
    let held = |claims: Capabilities| live.state.lock().capabilities = Some(RunCapabilities::new(run, claims));
    let said = async |by: RunId, prompt: u64| {
        let report = Report::Covering(Covering { prompt: Seq::new(prompt), line: "Hold on.".into() });
        live.send_item(Item::Report { run: by, report, seq: None, held_to_claims: true }).await;
        flushed(&live).await;
        live.snapshot().tail.covering.map(|covering| (covering.run, covering.prompt))
    };
    let mut cover_lines = Capabilities::default();
    cover_lines.output.cover_lines = Some(Claimed {});
    held(cover_lines);

    assert_eq!(said(run, 4).await, None, "conversation mode is off");

    live.start_conversation(ClientDevice { id: "d1".into(), name: "Mac Studio".into() }).await.unwrap();

    assert_eq!(said(RunId::new(1), 5).await, None, "a replaced run's");
    assert_eq!(said(run, 6).await, Some((Some(run), Seq::new(6))));
    held(Capabilities::default());
    assert_eq!(said(run, 7).await, Some((Some(run), Seq::new(6))), "a run that doesn't claim them, as Codex's doesn't");
}
