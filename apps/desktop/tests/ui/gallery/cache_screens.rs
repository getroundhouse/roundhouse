//! The screens about a chat's prompt cache, which the gallery takes after the compactions': the newest turn's card while
//! the cache is warm and once it has expired, the chat as it's about to expire and once it has, a prompt that found it
//! expired, and Home while one is about to.

#![cfg(test)]

use look::Region;
use protocol::event::EventBody;
use protocol::model::{Seq, SessionId, SessionStatus};

use super::chats::{self, finished};
use super::events::{replied, taken, text, user_prompt};
use super::frame::{CHAT, Frame, Part, TURN_CARD};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::world::World;
use super::worlds::{chat_world, conversation, titled};

/// The screens about the cache.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "turn-card-cache",
        spec: "chats.md",
        world: Some(|| cached(conversation(), 18)),
        frame: TURN_CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id(chats::FINISHED_TURN_END), shows: Region::TurnCard.id() },
        ],
    },
    Screen {
        name: "turn-card-cache-expired",
        spec: "chats.md",
        world: Some(|| cached(conversation(), 72)),
        frame: TURN_CARD,
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id(chats::FINISHED_TURN_END), shows: Region::TurnCard.id() },
        ],
    },
    Screen {
        name: "cache-expiring",
        spec: "chats.md",
        world: Some(|| cached(conversation(), 51)),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "cache-expired",
        spec: "chats.md",
        world: Some(|| cached(conversation(), 72)),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "cache-expired-rule",
        spec: "chats.md",
        world: Some(came_back),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "home-cache",
        spec: "home.md",
        world: Some(|| cached(titled(), 51)),
        frame: Frame::Exactly(Part::Region(Region::Home), &[]),
        steps: &[Step::Select(FOCUS), Step::Click("home")],
    },
];

/// `world`, with the newest turn of the screen's chat ending this many minutes before the picture, as
/// [`World::cached`] says.
fn cached(mut world: World, minutes_before_now: i64) -> World {
    world.cached(&shown_chat(&world), minutes_before_now);
    world
}

/// The chat each of these screens shows: the newest with anything in it.
fn shown_chat(world: &World) -> SessionId {
    world.sessions.iter().rfind(|session| session.last_seq > 0).unwrap().id.clone()
}

/// How long the chat had been idle when the prompt that found the cache expired came.
const IDLE_MS: i64 = 80 * 60_000;

/// The conversation, then a prompt sent after the chat had been idle for an hour and twenty minutes, which wrote it to
/// the cache again, and the reply.
fn came_back() -> World {
    let mut events = finished();
    let prompt = Seq::try_from(events.len()).unwrap() + 1;
    let reply = "Yes. The badge reads the state from the workspace's record, so it's right as soon as the feed says.";
    events.extend([
        user_prompt("Does the badge update when the PR merges?"),
        taken(prompt),
        EventBody::CacheExpired { idle_ms: IDLE_MS.unsigned_abs(), read_tokens: 14_755, written_tokens: 298_745 },
        text("msg-4", reply),
        replied(reply, 9, 0.62),
    ]);
    let mut world = chat_world(SessionStatus::Idle, events);
    world.idle_before(&shown_chat(&world), prompt, IDLE_MS);
    world
}
