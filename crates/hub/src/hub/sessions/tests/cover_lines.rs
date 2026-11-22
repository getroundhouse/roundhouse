//! The line said for the agent while it can't answer what was said out loud, and the mode it goes with.

use protocol::capability::output::{CoverLine, CoverWait};
use protocol::capability::{Capabilities, Claimed, Report};
use protocol::event::EventBody;
use protocol::model::{ClientDevice, SessionId};
use protocol::prompt::Prompt;

use std::sync::Arc;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};
use crate::hub::tests::flushed;
use crate::session::Item;

fn device(id: &str) -> ClientDevice {
    ClientDevice { id: id.into(), name: format!("Mac {id}") }
}

/// How the mode leaves the device that had the conversation.
#[derive(Clone, Copy, Debug)]
enum Leaving {
    TurnedOff,
    Typed,
    TakenOver,
}

/// The line the chat's tail has.
async fn said(hub: &Arc<Hub>, chat: &SessionId) -> Option<CoverLine> {
    let live = hub.live_session(chat).await.unwrap();
    flushed(&live).await;
    live.snapshot().tail.covering.map(|covering| covering.line)
}

/// The line goes with the mode, however it ends, since nobody is listening for it, and stays when another device takes
/// the conversation over, which says it from then on.
#[tokio::test]
async fn the_line_goes_with_the_mode_and_stays_when_the_conversation_moves() {
    for leaving in [Leaving::TurnedOff, Leaving::Typed, Leaving::TakenOver] {
        let dir = tempfile::tempdir().unwrap();
        let (hub, asked) = hub_with_stand_in_workers(dir.path());
        let (_, chat) = a_chat(&hub).await;
        a_turn(&hub, &asked, &chat.id, Prompt::new("look into it"), None).await;
        let mut claims = Capabilities::default();
        claims.output.cover_lines = Some(Claimed {});
        let init = EventBody::AgentInit {
            capabilities: Some(Box::new(claims)),
            agent_session_id: None,
            model: None,
            cwd: None,
            mode: None,
            agent_version: None,
        };
        the_agent_says(&hub, &asked, &chat.id, [init]).await;
        hub.session_conversation_start(chat.id.clone(), device("d1")).await.unwrap();
        let run = asked.launches(&chat.id).pop().unwrap().run;
        let line = CoverLine { wait: CoverWait(1), number: 1, text: "Hold on.".into() };
        let report = Report::Covering(line.clone());
        hub.route(&chat.id, Item::Report { run, report, seq: None }).await;
        assert_eq!(said(&hub, &chat.id).await, Some(line.clone()));

        let left = match leaving {
            Leaving::TurnedOff => hub.session_conversation_end(chat.id.clone()).await.map(drop),
            Leaving::Typed => {
                hub.session_prompt(chat.id.clone(), None, Prompt::new("never mind"), None).await.map(drop)
            }
            Leaving::TakenOver => hub.session_conversation_start(chat.id.clone(), device("d2")).await.map(drop),
        };
        left.unwrap();

        let stays = matches!(leaving, Leaving::TakenOver);
        assert_eq!(said(&hub, &chat.id).await, stays.then_some(line), "{leaving:?}");
    }
}
