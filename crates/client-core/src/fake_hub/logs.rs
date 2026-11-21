//! What a fake hub sends of a chat's log: a read of its events, a subscription to them and what's streaming, and its
//! prompts, each from what the world holds.

use std::collections::HashMap;
use std::convert::Infallible;

use protocol::client::log::ReadLimit;
use protocol::client::{ClientResponse, HubFrame};
use protocol::event::SessionEvent;
use protocol::model::{Place, PromptLine, Seq, SessionId, SettledAt, SubagentId};

use super::serve::{Answer, Outcome};
use super::world::World;

/// The log from `from_seq`, then the streaming tail, as a real hub sends them once it has caught up.
pub(super) fn subscribe(world: &World, id: &SessionId, from_seq: Seq, subscription_id: u64) -> Answer {
    world.session(id)?;
    let events = world.events_from(id, from_seq).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = HubFrame::Tail { subscription_id, tail: world.tails.get(id).cloned().unwrap_or_default() };
    Ok(Outcome::Reply(ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
}

/// The same, for the agent's own events or one subagent's, with only that one's part of what's still streaming.
pub(super) fn subscribe_places(
    world: &World,
    id: &SessionId,
    subagent: Option<&SubagentId>,
    from: Place,
    subscription_id: u64,
) -> Answer {
    world.session(id)?;
    let events =
        world.placed_from(id, subagent, from).map(|event| HubFrame::Event { subscription_id, event: event.clone() });
    let tail = world.tails.get(id).map_or_default(|tail| tail.of(subagent));
    let tail = HubFrame::Tail { subscription_id, tail };
    Ok(Outcome::Reply(ClientResponse::Ok, events.chain(std::iter::once(tail)).collect()))
}

/// What a read of `events` is sent, by what each takes in a hub's log: its body as JSON, and its raw line.
pub(super) fn read<'a>(limit: ReadLimit, events: impl Iterator<Item = &'a SessionEvent>) -> Vec<SessionEvent> {
    let sized = events.map(|event| {
        let size = serde_json::to_string(&event.body).unwrap().len() + event.raw.as_ref().map_or(0, String::len);
        Ok::<_, Infallible>((event.clone(), u64::try_from(size).unwrap()))
    });
    let Ok(sent) = limit.cut(sized);
    sent
}

/// Each prompt in the chat as a line, oldest first, with where it was settled, which is the last event to settle it.
pub(super) fn prompt_lines(world: &World, id: &SessionId) -> Vec<PromptLine> {
    let settled: HashMap<Seq, SettledAt> = world.events_from(id, Seq::new(1)).filter_map(SettledAt::of).collect();
    let line = |line: PromptLine| match settled.get(&line.seq()) {
        Some(at) => line.settled_at(*at),
        None => line,
    };
    world.events_from(id, Seq::new(1)).filter_map(PromptLine::of).map(line).collect()
}
