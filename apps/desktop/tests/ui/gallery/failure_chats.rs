//! A chat whose turn failed on a request to the model, and the screen of it, which the gallery takes after the
//! compactions'.

#![cfg(test)]

use protocol::event::{EventBody, FailureKind, NoticeLevel, RequestFailure, TurnOutcome};
use protocol::model::SessionStatus;

use super::chats::looked_around;
use super::events::{finished_turn, text};
use super::frame::CHAT;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::{NOW_MS, World};
use super::worlds::chat_world;

/// The screens about a request that failed.
pub(crate) const SCREENS: &[Screen] = &[Screen {
    name: "request-failed",
    spec: "chats.md",
    frame: CHAT,
    world: Some(failed_world),
    steps: &[Step::Select(FOCUS)],
}];

/// What the CLI says when the account's usage limit refuses a request, made up with an address, as the real one's has.
pub(crate) const LIMIT_REACHED: &str = "You've reached your usage limit. Switch to another model, or manage usage at https://example.com/usage, to continue.";

/// The chat, which the hub says stopped short on the turn that failed, so its card offers to try again.
fn failed_world() -> World {
    let mut world = chat_world(SessionStatus::Idle, failed());
    let chat = world.sessions.last_mut().unwrap();
    chat.reply_seq = chat.last_seq;
    chat.reply_outcome = Some(TurnOutcome::Error);
    world
}

/// A turn that looked around, had its model changed partway, which the CLI said itself, and then failed on the
/// account's usage limit, which resets in four days and six hours.
fn failed() -> Vec<EventBody> {
    let mut events = looked_around();
    let model_set = "Set model to `opus (claude-opus-5-5)`".to_string();
    events.push(EventBody::Notice { level: NoticeLevel::Info, message: model_set, parent_tool_use_id: None });
    events.push(text("msg-3", "The sidebar only knows what's in `Workspace`, so I'll add the field there first."));
    let failure = RequestFailure {
        message: LIMIT_REACHED.to_string(),
        kind: FailureKind::Limit,
        resets_at_ms: Some(NOW_MS + (4 * 24 + 6) * 60 * 60 * 1_000),
    };
    events.push(EventBody::RequestFailed { parent_tool_use_id: None, failure });
    let mut ended = finished_turn(74, 0.18);
    if let EventBody::TurnEnded { outcome, result, request_failed, .. } = &mut ended {
        (*outcome, *result, *request_failed) = (TurnOutcome::Error, Some(LIMIT_REACHED.to_string()), true);
    }
    events.push(ended);
    events
}
