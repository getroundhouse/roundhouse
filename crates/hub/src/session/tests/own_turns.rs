//! A turn the agent started by itself, which the hub records with what it keeps of each wait the agent heard had ended.

use super::super::Item;
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker};
use crate::store::Seen;
use protocol::capability::turns::OwnTurn;
use protocol::event::{AgentTask, EndedTask, EventBody, TaskKind, TaskOutcome};
use protocol::model::{EndedWait, RunId, Seq, WaitEnd, WaitEnded, WaitUntil, Watching};

fn ended(id: &str, kind: TaskKind, call: &str) -> EndedTask {
    let task = AgentTask { id: id.into(), description: None, kind, call: Some(call.into()), subagent: None };
    EndedTask { task, outcome: TaskOutcome::Finished, wait: None }
}

/// The agent heard only the tool's answer, so a wait it names by its call carries what the hub keeps of it, which is
/// what it watched and how it ended. Any other kind of task, and a wait the hub has no record of, carry none, whatever
/// the agent said.
#[tokio::test]
async fn a_turn_the_agent_started_by_itself_carries_the_hubs_record_of_each_wait_that_ended() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = stand_in_worker(&hub);
    let run = RunId::new(1);
    let live = carried_on(&hub, &link, run);
    let mut wait = protocol::fixtures::pull_request_wait("toolu_w");
    (wait.id, wait.session_id, wait.workspace_id) = ("a".into(), "s".into(), "w".into());
    let ended_at = WaitEnded { at_ms: 2, how: WaitEnd::ChecksPassed };
    hub.blocking(move |store| {
        store.start_wait(&wait, Seen::default()).and_then(|_| store.end_wait(&"a".into(), ended_at))
    })
    .await
    .unwrap();
    let said = EndedWait {
        id: "made-up".into(),
        watching: Watching::Branch { number: 9, until: WaitUntil::Merged },
        ended: None,
    };
    let mut command = ended("b2", TaskKind::Command, "toolu_w");
    command.wait = Some(said);
    let heard = vec![ended("k1", TaskKind::Wait, "toolu_w"), command, ended("k3", TaskKind::Wait, "toolu_elsewhere")];
    let body = EventBody::OwnTurn(OwnTurn::TaskEnded { ended: heard });

    live.send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None }).await;
    flushed(&live).await;

    let events = hub.blocking(|store| store.read_events(&"s".into(), Seq::FIRST, 1000, u64::MAX, false)).await.unwrap();
    let Some(EventBody::OwnTurn(OwnTurn::TaskEnded { ended })) = events.into_iter().map(|event| event.body).next_back()
    else {
        panic!("an own turn, recorded")
    };
    let kept: Vec<_> = ended.iter().map(|task| task.wait.clone()).collect();
    let watching = Watching::PullRequests { numbers: vec![7], until: WaitUntil::RequiredChecks };
    let hubs = EndedWait { id: "a".into(), watching, ended: Some(ended_at) };
    assert_eq!(kept, [Some(hubs), None, None]);
}
