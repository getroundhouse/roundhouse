//! The run is told how the person talks with the agent: as it launches, and as conversation mode is turned on or off,
//! if it claims to say lines for the agent.

use std::sync::Arc;

use protocol::capability::{Capabilities, Claimed, Control};
use protocol::event::EventBody;
use protocol::model::{ClientDevice, InteractionMode, SessionId};
use protocol::prompt::Prompt;
use protocol::worker::WorkerRequest;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};

fn device(id: &str) -> ClientDevice {
    ClientDevice { id: id.into(), name: format!("Mac {id}") }
}

/// Each mode the stand-ins were asked to tell the agent of `chat`, in order.
fn told(asked: &Asked, chat: &SessionId) -> Vec<InteractionMode> {
    asked.requests(|request| {
        let WorkerRequest::AgentControl(asked) = request else { return None };
        let Control::Interaction { mode } = asked.control else { return None };
        (asked.session_id == *chat).then_some(mode)
    })
}

/// Once the stand-ins have been asked to tell the agent of `chat` as many modes as `modes` has, which they are in.
async fn told_as(asked: &Asked, chat: &SessionId, modes: &[InteractionMode]) -> Vec<InteractionMode> {
    waiting::task::until(TIMEOUT, "the agent told the mode", || told(asked, chat).len() >= modes.len()).await;
    told(asked, chat)
}

/// The agent of `chat` says it can say lines for itself, as Claude's will.
async fn claims_cover_lines(hub: &Arc<Hub>, asked: &Asked, chat: &SessionId) {
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
    the_agent_says(hub, asked, chat, [init]).await;
}

/// The mode is told only as it's turned on or off, however that happens, and not as the conversation moves, which
/// leaves it on, or to a run that doesn't claim to say lines, which has no use for it.
#[tokio::test]
async fn the_run_is_told_as_the_mode_is_turned_on_or_off_if_it_says_lines() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("look into it"), None).await;
    let id = chat.id.clone();
    let (on, off) = (InteractionMode::Conversation, InteractionMode::Typing);

    assert_eq!(asked.launches(&id).last().map(|spec| spec.interaction), Some(off));
    hub.session_conversation_start(id.clone(), device("d1")).await.unwrap();
    hub.session_conversation_end(id.clone()).await.unwrap();
    claims_cover_lines(&hub, &asked, &id).await;
    hub.session_conversation_start(id.clone(), device("d1")).await.unwrap();
    assert_eq!(told_as(&asked, &id, &[on]).await, [on], "only once it claims them");

    hub.session_conversation_start(id.clone(), device("d1")).await.unwrap();
    hub.session_conversation_start(id.clone(), device("d2")).await.unwrap();
    hub.session_conversation_end(id.clone()).await.unwrap();
    assert_eq!(told_as(&asked, &id, &[on, off]).await, [on, off], "moving it says nothing");

    hub.session_conversation_start(id.clone(), device("d2")).await.unwrap();
    hub.session_prompt(id.clone(), None, Prompt::new("never mind"), None).await.unwrap();
    assert_eq!(told_as(&asked, &id, &[on, off, on, off]).await, [on, off, on, off], "typing turns it off");
}

/// An agent launched while the mode is on is told so as it starts.
#[tokio::test]
async fn an_agent_launched_in_the_mode_is_told_so_as_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;

    hub.session_conversation_start(chat.id.clone(), device("d1")).await.unwrap();
    let said = Prompt::new("what's in the parser?").said_out_loud();
    hub.session_prompt(chat.id.clone(), None, said, Some(device("d1"))).await.unwrap();
    waiting::task::until(TIMEOUT, "the agent launched", || !asked.launches(&chat.id).is_empty()).await;

    let launched = asked.launches(&chat.id).pop().unwrap();
    assert_eq!(launched.interaction, InteractionMode::Conversation);
}
