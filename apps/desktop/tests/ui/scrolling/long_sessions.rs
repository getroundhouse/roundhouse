//! Long sessions to scroll through: made by the fake agent on a real hub, for a test of how the app pages through
//! what a hub holds, or held by a fake hub, for a test of what the window draws of one that's loaded.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, taken, text, user_prompt};
use gpui_kit::TestAppContext;
use protocol::event::SessionEvent;
use protocol::model::{Seq, SessionStatus};

use super::wait_for_the_list_to_settle;
use crate::support::{FakeHub, Stack, Ui};

/// How many things the agent says in a long session, which is more than a transcript loads at once.
pub(super) const LONG: usize = 700;

/// How many things the agent is asked to say in a prompt, as a person would ask, so no prompt is itself most of the
/// session.
const TO_A_PROMPT: usize = 25;

/// The agent's `i`th reply in a long session: a line, a paragraph or a few screens, in turn.
fn long_reply(i: usize) -> String {
    "the signal box keeps a ledger. ".repeat([1, 4, 30][i % 3])
}

/// A long session, open at its end with the scrollbar showing, and everything loaded so far measured.
pub(super) fn open_on_a_long_session<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let newest = session_saying(stack, LONG, long_reply);
    let mut ui = Ui::open(cx, stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let last = ui.row_of(&newest);
    wait_for_the_list_to_settle(&mut ui, last);
    ui
}

/// The same session, held by a fake hub, and open as [`open_on_a_long_session`] opens it. The hub goes with the
/// test, so it's returned to be kept.
pub(super) fn open_on_a_long_chat(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let mut events = Vec::new();
    for said in Vec::from_iter(0..LONG).chunks(TO_A_PROMPT) {
        let prompt = Seq::new(u64::try_from(events.len() + 1).unwrap());
        events.extend([user_prompt("Keep the ledger."), taken(prompt)]);
        events.extend(said.iter().map(|&i| text(&format!("msg-{i}"), &long_reply(i))));
        events.push(finished_turn(12, 0.04));
    }
    let chat = world.chat(&workspace, SessionStatus::Idle, events);
    let newest = world.events_from(&chat, Seq::default()).last().cloned().unwrap();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let last = ui.row_of(&newest);
    wait_for_the_list_to_settle(&mut ui, last);
    (fake, ui)
}

/// Starts a session and has the agent say `count` things in it, over as many prompts as that takes. Returns the newest
/// of the agent's own events in it, as the hub recorded it.
pub(super) fn session_saying(stack: &Stack, count: usize, say: impl Fn(usize) -> String) -> SessionEvent {
    session_of(stack, &saying(count, say))
}

/// The prompts that have the agent say `count` things.
fn saying(count: usize, say: impl Fn(usize) -> String) -> Vec<String> {
    // Each reply in one piece, since only what's recorded matters here, and streamed it took most of a test's time.
    prompts(count, |i| format!("whole:{}", say(i)))
}

/// `count` of the fake agent's commands, [`TO_A_PROMPT`] to a prompt.
pub(super) fn prompts(count: usize, step: impl Fn(usize) -> String) -> Vec<String> {
    Vec::from_fn(count, step).chunks(TO_A_PROMPT).map(|prompt| prompt.join("; ")).collect()
}

/// Starts a session and runs each of `prompts` in it. Returns the newest of the agent's own events in it, as the hub
/// recorded it.
pub(super) fn session_of(stack: &Stack, prompts: &[String]) -> SessionEvent {
    let session = stack.chat_in_a_new_workspace();
    for prompt in prompts {
        stack.run(&session, prompt);
    }
    stack.newest_event(&session)
}
