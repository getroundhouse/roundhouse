//! Finding text in a chat: `Cmd-F` opens a bar over it, which counts the matches in the whole chat, loaded or not, and
//! steps through them, opening what each is in, until `Escape` closes it.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use gpui_kit::test::ElementSnapshot;
use protocol::model::{SettledAt, seq_index};
use transcript::Transcript;

use crate::support::{Stack, Ui};

/// Longer than the bar waits for typing to pause.
const PAUSE: Duration = Duration::from_secs(1);
/// The fake agent's commands come with no description, so their line says they run something.
const COMMAND_LINE: &str = "Run";
/// What the fake agent's subagent is for, which its call's line says.
const SUBAGENT_LINE: &str = "Look around";

fn says(ui: &Ui<'_>) -> Option<String> {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.find_says())
}

fn found_row(ui: &Ui<'_>) -> Option<usize> {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.found_row())
}

fn command_open(ui: &mut Ui<'_>) -> Option<bool> {
    ui.labelled(COMMAND_LINE).first().and_then(ElementSnapshot::expanded)
}

#[gpui_kit::test]
fn cmd_f_finds_in_the_whole_chat_and_opens_what_a_match_is_in(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "tool:echo a-needle-printed");
    // Far more events than the transcript loads to start with, so the call isn't among them.
    let script: Vec<String> = (0..700).map(|i| format!("say:message {i}")).collect();
    stack.run(&session, &script.join("; "));
    stack.run(&session, "say:the last needle");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let (events, _) = stack.client().session_read(&session, 1, 50, false).unwrap();
    let call = events.iter().find(|e| matches!(&e.body, protocol::event::EventBody::Assistant { .. })).unwrap();
    let call_row = seq_index(call.seq) - 1;
    // The first prompt is drawn where the agent took it, once it had started.
    let took_the_first = |e: &&protocol::event::SessionEvent| SettledAt::of(e).is_some_and(|(p, _)| p == events[0].seq);
    let first_row = seq_index(events.iter().find(took_the_first).unwrap().seq) - 1;
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(call_row).is_none()), "the call isn't loaded");

    // With focus in the message box, as it is when a chat opens.
    ui.press("cmd-f");
    assert!(ui.has("find-bar"));
    ui.input("NEEDLE");
    ui.cx.executor().advance_clock(PAUSE);

    // Each prompt that says it, the command and what it printed, and the last reply. The first shown is the first
    // on screen, at the end of the chat.
    ui.wait("every match to be counted", |ui| says(ui).as_deref() == Some("4 of 5"));
    ui.press("enter");
    assert_eq!(says(&ui).as_deref(), Some("5 of 5"));
    ui.press("enter");
    assert_eq!(says(&ui).as_deref(), Some("1 of 5"), "round to the first, in the first prompt");
    assert_eq!(found_row(&ui), Some(first_row));
    ui.wait("the first prompt to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(first_row).is_some()));

    ui.press("enter");
    ui.wait("the call to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(call_row).is_some()));
    assert_eq!(says(&ui).as_deref(), Some("2 of 5"));
    assert_eq!(found_row(&ui), Some(call_row), "the command, in its call");
    ui.wait("the call to open", |ui| command_open(ui) == Some(true));
    // Its row was sent to the top, under the bar, and the list scrolls it down out from under it.
    ui.wait("the match to come into view below the bar", |ui| {
        ui.cx.read_entity(&transcript, |t, _| t.match_in_view()) == Some(true)
    });

    ui.press("cmd-shift-g");
    assert_eq!(says(&ui).as_deref(), Some("1 of 5"), "and back");

    ui.press("escape");
    assert!(!ui.has("find-bar"), "Escape closes the bar");
    assert_eq!(says(&ui), None);
    assert_eq!(command_open(&mut ui), Some(true), "and what it opened stays open");
}

#[gpui_kit::test]
fn what_s_typed_is_found_as_it_s_typed_and_the_bar_says_when_nothing_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:Here is the **parser** at work");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the chat", |ui| ui.transcript().is_some());

    ui.press("cmd-f");
    ui.input("parser at");
    ui.cx.executor().advance_clock(PAUSE);
    // In the prompt, which is Markdown too, and in the reply, both across the bold.
    ui.wait("the matches", |ui| says(ui).as_deref() == Some("1 of 2"));

    ui.input("x");
    ui.cx.executor().advance_clock(PAUSE);
    ui.wait("no match", |ui| says(ui).as_deref() == Some("No matches"));
    assert!(ui.disabled("find-next"), "with nothing to go to");
}

#[gpui_kit::test]
fn one_letter_is_found_only_in_what_s_loaded_and_the_count_says_so(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:the zebra crossing, which isn't loaded");
    let script: Vec<String> = (0..700).map(|i| format!("say:message {i}")).collect();
    stack.run(&session, &script.join("; "));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(1).is_none()), "the first reply isn't loaded");

    ui.press("cmd-f");
    ui.input("z");
    ui.cx.executor().advance_clock(PAUSE);
    ui.wait("the bar to say so", |ui| says(ui).is_some_and(|says| says.contains("No matches")));
    assert_eq!(says(&ui).as_deref(), Some("No matches loaded"), "though the first prompt and reply have one");

    ui.input("e");
    ui.cx.executor().advance_clock(PAUSE);
    ui.wait("the hub to find them", |ui| says(ui).as_deref() == Some("1 of 2"));
}

#[gpui_kit::test]
fn a_match_in_what_a_subagent_was_asked_opens_its_call_and_is_drawn_with_its_marks(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // The fake agent's subagent is asked to "Look around.", which nothing else in the chat says with the full stop.
    stack.run(&session, &format!("agent:{SUBAGENT_LINE}; say:done"));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    assert_eq!(ui.labelled(SUBAGENT_LINE).first().and_then(ElementSnapshot::expanded), Some(false));

    ui.press("cmd-f");
    ui.input("around.");
    ui.cx.executor().advance_clock(PAUSE);

    ui.wait("the match", |ui| says(ui).as_deref() == Some("1 of 1"));
    ui.wait("its call to open", |ui| {
        ui.labelled(SUBAGENT_LINE).first().and_then(ElementSnapshot::expanded) == Some(true)
    });
    // What it was asked is Markdown, whose matches only the view the transcript keeps of it draws.
    ui.wait("the match to be drawn in the view that marks it", |ui| {
        ui.cx.read_entity(&transcript, Transcript::match_in_its_view) == Some(true)
    });
}
