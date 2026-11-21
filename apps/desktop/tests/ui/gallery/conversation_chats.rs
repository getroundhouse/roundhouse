//! Chats in conversation mode, and the screens of how the chat shows it: the rule where the mode was turned on,
//! prompts said out loud, and the agent's replies as they were heard, read out loud. The same sidebar and parser as the
//! owner's pictures of it.

#![cfg(test)]

use protocol::event::EventBody;
use protocol::model::{
    ClientDevice, Ending, ReplyHeard, ReplyPlace, Rev, Seq, SessionId, SessionStatus, SettledBy, SpokenThrough,
};
use protocol::prompt::{Prompt, PromptFrom};
use protocol::tail::Tail;

use super::frame::CHAT;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::sidebar;
use client_core::fake_hub::World;
use client_core::fake_hub::events::{replied, taken, text, user_prompt};

/// The screens about conversation mode.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "conversation-heard",
        spec: "chats.md",
        world: Some(talked_over_and_speaking),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "conversation-not-read-out",
        spec: "chats.md",
        world: Some(stopped_and_not_read_out),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
];

/// The reply the person talked over, after its first sentence.
const TALKED_OVER: &str = "Sure, the parser agent's on it. For the sidebar, I'd keep the badge and let the name shrink \
                           first, then hide the badge only when the name's down to a few letters.";
/// The reply being read out, as far as `show`.
const SPEAKING: &str =
    "Below about two hundred pixels, I'd drop the badge and show just the mark. The name stays readable that way.";
/// The reply Stop stopped, partway into its second sentence.
const STOPPED: &str = "I'd call it the mark, since the sidebar already draws one for each workspace. The badge's \
                       count would move to the mark's tooltip, and the row keeps its room for the name.";
/// The reply never read out, since the mode ended before it was reached.
const NOT_READ_OUT: &str = "At narrow widths the row already drops the badge, so nothing changes there.";
/// The reply to the typed prompt, written rather than said, with Markdown, which a reply said out loud has none of.
const WRITTEN: &str = "Done. Below 200px the row draws `look::chat_mark` where the badge was:\n\n\
                       - The badge's count moves to the mark's tooltip.\n\
                       - `sidebar/row.rs` chooses between them by the row's width.";

/// The mode turned on, a prompt said out loud, a reply the person talked over, another said over it, and the reply to
/// that being read out.
fn talked_over_and_speaking() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let events = vec![
        turned_on(),
        said_out_loud("Can you get a subagent on the parser bug while we sort out the sidebar?"),
        taken(Seq::new(2)),
        text("msg-1", TALKED_OVER),
        replied(TALKED_OVER, 9, 0.04),
        said_out_loud("Wait, what about narrow windows? Does the badge still fit?"),
        taken(Seq::new(6)),
        text("msg-2", SPEAKING),
        replied(SPEAKING, 5, 0.02),
    ];
    let chat = world.chat(&focus, SessionStatus::Idle, events);
    let talked_over = Ending::TalkedOver { through: through(TALKED_OVER, "then") };
    world.replies_heard.insert(chat.clone(), vec![heard(&chat, "msg-1", talked_over)]);
    let speaking = SpokenThrough { message: "msg-2".into(), through: through(SPEAKING, "show") };
    world.tails.insert(chat, Tail { speaking: Some(speaking), ..Tail::default() });
    world
}

/// The mode turned on, a reply Stop stopped, the next never read out as the mode ended, and a prompt typed after it,
/// with no microphone, and its written reply.
fn stopped_and_not_read_out() -> World {
    let mut world = World::default();
    let focus = sidebar(&mut world);
    let events = vec![
        turned_on(),
        said_out_loud("What should we call the badge once it's smaller?"),
        taken(Seq::new(2)),
        text("msg-1", STOPPED),
        replied(STOPPED, 12, 0.03),
        said_out_loud("And in narrow windows?"),
        taken(Seq::new(6)),
        text("msg-2", NOT_READ_OUT),
        replied(NOT_READ_OUT, 4, 0.01),
        user_prompt("Do that. Use the mark the sidebar already draws, not a new icon."),
        taken(Seq::new(10)),
        text("msg-3", WRITTEN),
        replied(WRITTEN, 41, 0.11),
    ];
    let chat = world.chat(&focus, SessionStatus::Idle, events);
    let stopped = Ending::Stopped { through: through(STOPPED, "count") };
    let heard = vec![heard(&chat, "msg-1", stopped), heard(&chat, "msg-2", Ending::NotReadOut)];
    world.replies_heard.insert(chat, heard);
    world
}

/// The person turning conversation mode on, on their Mac.
fn turned_on() -> EventBody {
    let device = ClientDevice { id: "mac-studio".into(), name: "Mac Studio".to_string() };
    EventBody::ConversationStarted { device }
}

/// A prompt the person said out loud.
fn said_out_loud(text: &str) -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new(text).said_out_loud(), from: PromptFrom::Person }
}

/// The place in a reply of one block, `reply`, just before `word`.
fn through(reply: &str, word: &str) -> ReplyPlace {
    ReplyPlace { block: 0, through: u32::try_from(reply.find(word).unwrap()).unwrap() }
}

/// How the reply `message` in `chat` ended, as the device that read it said.
fn heard(chat: &SessionId, message: &str, ending: Ending) -> ReplyHeard {
    ReplyHeard {
        session_id: chat.clone(),
        message: message.into(),
        ending,
        settled_by: SettledBy::Device,
        rev: Rev::new(1),
    }
}
