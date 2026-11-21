//! Long sessions to scroll through: made by the fake agent on a real hub, for a test of how the app pages through
//! what a hub holds, or held by a fake hub, for a test of what the window draws of one that's loaded.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, text, turn_ending_with, user_prompt};
use gpui_kit::TestAppContext;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{Seq, SessionStatus};

use super::wait_for_the_list_to_settle;
use crate::support::{FakeHub, Stack, Ui};

/// How many things the agent says in a long session, which is more than a transcript loads at once.
pub(super) const LONG: usize = 700;

/// How many things the agent is asked to say in a prompt, as a person would ask, so no prompt is itself most of the
/// session.
const TO_A_PROMPT: usize = 25;

/// The agent's `i`th reply in a long session: a line, a paragraph or a few screens, in turn.
pub(super) fn long_reply(i: usize) -> String {
    "the signal box keeps a ledger. ".repeat([1, 4, 30][i % 3])
}

/// A long session, held by a fake hub, open at its end with the scrollbar showing, and everything loaded so far
/// measured. The hub goes with the test, so it's returned to be kept.
pub(super) fn open_on_a_long_chat(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>) {
    open_on_a_chat_saying(cx, LONG, long_reply)
}

/// A chat held by a fake hub where the agent has said `count` things, over as many prompts as that takes, open as
/// [`open_on_a_long_chat`] opens its own. The hub goes with the test, so it's returned to be kept.
pub(super) fn open_on_a_chat_saying(
    cx: &mut TestAppContext,
    count: usize,
    say: impl Fn(usize) -> String,
) -> (FakeHub, Ui<'_>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let mut events = Vec::new();
    for said in Vec::from_iter(0..count).chunks(TO_A_PROMPT) {
        let said = said.iter().map(|&i| text(&format!("msg-{i}"), &say(i)));
        turn_ending_with(&mut events, user_prompt("Keep the ledger."), said, finished_turn(12, 0.04));
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

/// A chat held by a fake hub of `events`, open as [`open_on_a_long_chat`] opens its own. Gives the newest event too,
/// to settle at again. The hub goes with the test, so it's returned to be kept.
pub(super) fn open_on_a_chat(cx: &mut TestAppContext, events: Vec<EventBody>) -> (FakeHub, Ui<'_>, SessionEvent) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let chat = world.chat(&workspace, SessionStatus::Idle, events);
    let newest = world.events_from(&chat, Seq::default()).last().cloned().unwrap();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let last = ui.row_of(&newest);
    wait_for_the_list_to_settle(&mut ui, last);
    (fake, ui, newest)
}

/// The events of a chat of `turns`, as the hub records them: each prompt, the agent taking it, what it did, and the end
/// of the turn.
pub(super) fn turns(turns: Vec<(String, Vec<EventBody>)>) -> Vec<EventBody> {
    let mut events = Vec::new();
    for (asked, did) in turns {
        turn_ending_with(&mut events, user_prompt(&asked), did, finished_turn(12, 0.04));
    }
    events
}

/// Paragraphs of `paragraph`, as few as make a text over 4 KB, which is parsed on another thread. For a test that needs
/// a long text only for that, since the window lays out every line of one it scrolls through, which in a debug build
/// on Linux is most of such a test's time.
pub(super) fn just_over_4_kb(paragraph: &str) -> String {
    let text = vec![paragraph; 4 * 1024 / (paragraph.len() + 2) + 1].join("\n\n");
    assert!(text.len() > 4 * 1024, "parsed on another thread");
    text
}

/// `paragraphs` paragraphs of made-up prose, the same for the same `seed`, of lengths as a reply's vary: some a line,
/// some several, with now and then a list or a block of code, as the fake agent's `prose:` writes them.
pub(super) fn prose(paragraphs: usize, seed: usize) -> String {
    const WORDS: [&str; 16] = [
        "signal", "box", "keeps", "the", "ledger", "of", "every", "train", "that", "passes", "and", "lever", "frame",
        "holds", "route", "clear",
    ];
    let mut state = u64::try_from(seed).unwrap();
    let mut next = move |n: usize| {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).unwrap() % n
    };
    let paragraph = |_| {
        let words = [6, 14, 30, 60, 120][next(5)];
        let text = Vec::from_fn(words, |_| WORDS[next(16)]).join(" ");
        match next(8) {
            0 => format!("- {}\n- {text}\n- {}\n\n", WORDS[next(16)], WORDS[next(16)]),
            1 => format!("```rust\nfn {}() {{\n    {text}\n}}\n```\n\n", WORDS[next(16)]),
            _ => format!("{text}.\n\n"),
        }
    };
    Vec::from_fn(paragraphs, paragraph).concat()
}
