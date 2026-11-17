//! Chats made up for a fake hub to hold, for a test of what's selected of what the window draws, which needs no hub,
//! worker or agent.

use client_core::fake_hub::World;
use client_core::fake_hub::events::{command, finished_turn, printed, taken, text, user_prompt};
use gpui_kit::{Bounds, Pixels, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{Seq, SessionId, SessionStatus};

use super::super::wait_for_the_list_to_settle;
use super::bounds;
use crate::support::{FakeHub, Ui};

/// Something the agent did in a turn.
pub(super) enum Said {
    /// Wrote this, as a reply of its own.
    Reply(String),
    /// Ran this command, which printed what follows it.
    Command(&'static str, &'static str),
}

/// A turn: what the person asked, and what the agent did about it.
pub(super) type Turn = (String, Vec<Said>);

/// The chats of one workspace, open on the first, kept to its end, once its list has settled there. The hub goes with
/// the test, so it's returned to be kept, and so is each chat, and where the transcript is.
pub(super) fn open_on_chats(
    cx: &mut TestAppContext,
    chats: Vec<Vec<Turn>>,
) -> (FakeHub, Ui<'_>, Vec<SessionId>, Bounds<Pixels>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let ids = Vec::from_iter(chats.into_iter().map(|turns| world.chat(&workspace, SessionStatus::Idle, events(turns))));
    let newest = world.events_from(&ids[0], Seq::default()).last().cloned().unwrap();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    ui.activate_chat(&ids[0]);
    let last = ui.row_of(&newest);
    wait_for_the_list_to_settle(&mut ui, last);
    let window = bounds(&mut ui, "transcript".into());
    (fake, ui, ids, window)
}

/// The same, for one chat of `turns`.
pub(super) fn open_on_a_chat(cx: &mut TestAppContext, turns: Vec<Turn>) -> (FakeHub, Ui<'_>, Bounds<Pixels>) {
    let (fake, ui, _, window) = open_on_chats(cx, vec![turns]);
    (fake, ui, window)
}

/// The events of a chat of `turns`, as the hub records them: each prompt, the agent taking it, what it did about it,
/// each reply and each call its own message, and the end of the turn.
fn events(turns: Vec<Turn>) -> Vec<EventBody> {
    let mut events = Vec::new();
    for (asked, said) in turns {
        let prompt = Seq::new(u64::try_from(events.len() + 1).unwrap());
        events.extend([user_prompt(&asked), taken(prompt)]);
        for said in said {
            let message = format!("msg-{}", events.len());
            match said {
                Said::Reply(reply) => events.push(text(&message, &reply)),
                Said::Command(line, output) => {
                    let call = format!("toolu-{}", events.len());
                    events.push(command(&message, &call, line, None));
                    events.push(printed(&call, output));
                }
            }
        }
        events.push(finished_turn(12, 0.04));
    }
    events
}

/// A reply of `paragraphs` paragraphs, each of a few lines, which `seed` tells apart from the others.
pub(super) fn prose(paragraphs: usize, seed: usize) -> Said {
    let paragraph = |n| {
        format!(
            "Ledger {seed}, entry {n}: the signal box keeps a ledger of every train that passes, and the lever frame \
             holds each route clear until the train has gone by and the block ahead of it is clear again."
        )
    };
    Said::Reply(Vec::from_fn(paragraphs, paragraph).join("\n\n"))
}

/// A reply that says `text`.
pub(super) fn reply(text: &str) -> Said {
    Said::Reply(text.to_owned())
}
