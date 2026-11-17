//! Finding text in a chat: `Cmd-F`, or `Ctrl-F` off the Mac, opens a bar over it, which counts the matches in the
//! whole chat, loaded or not, and steps through them, opening what each is in, until `Escape` closes it. What a chat
//! holds, and which of its events hold what's typed, is the hub's, and what the bar shows of it the app's, so each
//! window opens on a fake hub holding a made-up chat, with no worker or agent to wait for.

#![cfg(test)]

use std::time::Duration;

use client_core::fake_hub::World;
use client_core::fake_hub::events::{
    command, finished_turn, launched, printed, subagent, subagent_ended, subagent_started, taken, text, user_prompt,
    within,
};
use gpui_kit::AppContext as _;
use gpui_kit::test::ElementSnapshot;
use gpui_kit::{ScrollDelta, TestAppContext, point, px};
use protocol::capability::calls::call::Runs;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{Seq, SessionStatus, SettledAt};

use crate::support::{FakeHub, Ui};

/// Longer than the bar waits for typing to pause.
const PAUSE: Duration = Duration::from_secs(1);
/// A command with no description, whose line says it runs something.
const COMMAND_LINE: &str = "Run";
/// What the subagent is for, which its call's line says.
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

/// A turn as the hub records one: the prompt, the agent taking it, what the agent did, and the turn's end.
fn turn(events: &mut Vec<EventBody>, prompt: &str, then: impl IntoIterator<Item = EventBody>) {
    let seq = Seq::new(u64::try_from(events.len()).unwrap() + 1);
    events.extend([user_prompt(prompt), taken(seq)]);
    events.extend(then);
    events.push(finished_turn(3, 0.01));
}

/// A turn in which the agent says `count` messages, each of its own, whose words are only `message` and a number.
fn messages(events: &mut Vec<EventBody>, count: usize) {
    turn(events, "Count.", Vec::from_fn(count, |i| text(&format!("count-{i}"), &format!("message {i}"))));
}

/// A world of one workspace, with a chat holding `events`, and the chat's first `loaded` events as the hub holds them.
fn chat_of(events: Vec<EventBody>, loaded: usize) -> (World, Vec<SessionEvent>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let chat = world.chat(&workspace, SessionStatus::Idle, events);
    let held = world.events_from(&chat, Seq::new(1)).take(loaded).cloned().collect();
    (world, held)
}

/// The window on a fake hub holding `world`, showing its chat. The hub goes with the test, so it's returned to be kept.
fn open(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    (fake, ui)
}

/// Opens the bar and types `query` into it, then lets the pause the bar waits for go by.
fn find(ui: &mut Ui<'_>, query: &str) {
    ui.press("secondary-f");
    assert!(ui.has("find-bar"));
    ui.input(query);
    ui.cx.executor().advance_clock(PAUSE);
}

#[gpui_kit::test]
fn cmd_f_finds_in_the_whole_chat_and_opens_what_a_match_is_in(cx: &mut TestAppContext) {
    let mut events = Vec::new();
    turn(
        &mut events,
        "Echo a needle.",
        [command("echo", "toolu-1", "echo a-needle-printed", None), printed("toolu-1", "a-needle-printed")],
    );
    // Far more events than the transcript loads to start with, so the call isn't among them.
    messages(&mut events, 700);
    turn(&mut events, "Say the last needle.", [text("last", "the last needle")]);
    let (world, events) = chat_of(events, 50);
    let (_hub, mut ui) = open(cx, world);
    let transcript = ui.transcript().unwrap();
    let call = events.iter().find(|e| matches!(&e.body, EventBody::Assistant { .. })).unwrap();
    let call_row = ui.row_of(call);
    // The first prompt is drawn where the agent took it.
    let took_the_first = |e: &&SessionEvent| SettledAt::of(e).is_some_and(|(p, _)| p == events[0].seq);
    let first_row = ui.row_of(events.iter().find(took_the_first).unwrap());
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(call_row).is_none()), "the call isn't loaded");

    // With focus in the message box, as it is when a chat opens.
    find(&mut ui, "NEEDLE");

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

    ui.press("secondary-shift-g");
    assert_eq!(says(&ui).as_deref(), Some("1 of 5"), "and back");

    ui.press("escape");
    assert!(!ui.has("find-bar"), "Escape closes the bar");
    assert_eq!(says(&ui), None);
    assert_eq!(command_open(&mut ui), Some(true), "and what it opened stays open");
}

/// The agent's words, which the transcript draws from the kit's blocks, say where they drew the match being shown, so
/// one far up the chat is brought into view below the bar. What's counted in them is what they show, and not a link's
/// address.
#[gpui_kit::test]
fn a_match_in_a_reply_far_up_the_chat_is_brought_into_view_below_the_bar(cx: &mut TestAppContext) {
    let mut events = Vec::new();
    let reply = "The **needle** is in [this reply](https://example.com/needle), once";
    turn(&mut events, "Where's the needle?", [text("found", reply)]);
    // Enough replies after it, each a few short paragraphs, to put it well above the window, which is all this needs:
    // longer ones would only cost time to lay out.
    let paragraphs = |reply: usize| Vec::from_fn(5, |part| format!("Reply {reply}, part {part}.")).join("\n\n");
    turn(&mut events, "Go on.", Vec::from_fn(30, |reply| text(&format!("on-{reply}"), &paragraphs(reply))));
    let (world, _) = chat_of(events, 0);
    let (_hub, mut ui) = open(cx, world);
    let transcript = ui.transcript().unwrap();

    find(&mut ui, "needle");

    // Once in the prompt that asked for it and once in the reply, in its words, and not in the link's address.
    ui.wait("the matches to be counted", |ui| says(ui).as_deref() == Some("1 of 2"));
    ui.press("enter");
    assert_eq!(says(&ui).as_deref(), Some("2 of 2"));
    let row = found_row(&ui).unwrap();
    let in_a_reply = |ui: &Ui<'_>| {
        ui.cx.read_entity(&transcript, |t, _| {
            t.event_at(row).is_some_and(|event| matches!(event.body, EventBody::Assistant { .. }))
        })
    };
    ui.wait("the reply to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_some()));
    assert!(in_a_reply(&ui), "the second is in the reply");
    ui.wait("the match to come into view below the bar", |ui| {
        ui.cx.read_entity(&transcript, |t, _| t.match_in_view()) == Some(true)
    });
}

/// As Windows and Linux apps have them, besides Ctrl-G and Ctrl-Shift-G.
#[cfg(not(target_os = "macos"))]
#[gpui_kit::test]
fn off_the_mac_f3_and_shift_f3_step_through_the_matches_too(cx: &mut TestAppContext) {
    let mut events = Vec::new();
    turn(&mut events, "Say a needle.", [text("said", "a needle")]);
    let (world, _) = chat_of(events, 0);
    let (_hub, mut ui) = open(cx, world);
    find(&mut ui, "needle");
    // In the prompt and in the reply.
    ui.wait("the matches", |ui| says(ui).as_deref() == Some("1 of 2"));

    ui.press("f3");
    assert_eq!(says(&ui).as_deref(), Some("2 of 2"));
    ui.press("shift-f3");
    assert_eq!(says(&ui).as_deref(), Some("1 of 2"));
}

#[gpui_kit::test]
fn what_s_typed_is_found_as_it_s_typed_and_the_bar_says_when_nothing_is(cx: &mut TestAppContext) {
    let mut events = Vec::new();
    turn(&mut events, "Is the **parser** at work?", [text("said", "Here is the **parser** at work")]);
    let (world, _) = chat_of(events, 0);
    let (_hub, mut ui) = open(cx, world);

    find(&mut ui, "parser at");
    // In the prompt, which is Markdown too, and in the reply, both across the bold.
    ui.wait("the matches", |ui| says(ui).as_deref() == Some("1 of 2"));

    ui.input("x");
    ui.cx.executor().advance_clock(PAUSE);
    ui.wait("no match", |ui| says(ui).as_deref() == Some("No matches"));
    assert!(ui.off("find-next"), "with nothing to go to");
}

#[gpui_kit::test]
fn one_letter_is_found_only_in_what_s_loaded_and_the_count_says_so(cx: &mut TestAppContext) {
    let mut events = Vec::new();
    turn(&mut events, "Where's the zebra crossing?", [text("said", "the zebra crossing, which isn't loaded")]);
    messages(&mut events, 700);
    let (world, _) = chat_of(events, 0);
    let (_hub, mut ui) = open(cx, world);
    let transcript = ui.transcript().unwrap();
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(1).is_none()), "the first reply isn't loaded");

    find(&mut ui, "z");
    ui.wait("the bar to say so", |ui| says(ui).is_some_and(|says| says.contains("No matches")));
    assert_eq!(says(&ui).as_deref(), Some("No matches loaded"), "though the first prompt and reply have one");

    ui.input("e");
    ui.cx.executor().advance_clock(PAUSE);
    ui.wait("the hub to find them", |ui| says(ui).as_deref() == Some("1 of 2"));
}

#[gpui_kit::test]
fn a_match_in_what_a_subagent_was_asked_opens_its_call_and_is_drawn_with_its_marks(cx: &mut TestAppContext) {
    // The subagent is asked to "Look around.", which nothing else in the chat says with the full stop, and it works
    // in the background, as the CLI's do, so its call has lines of its own to open to.
    let asked = format!("{SUBAGENT_LINE}.");
    let mut events = Vec::new();
    turn(
        &mut events,
        "Send a subagent.",
        [
            subagent("send", "toolu-1", "Explore", SUBAGENT_LINE, &asked, Runs::Background),
            subagent_started("toolu-1", "Explore", SUBAGENT_LINE, &asked),
            launched("toolu-1"),
            within("toolu-1", command("ls", "toolu-2", "ls", None)),
            within("toolu-1", printed("toolu-2", "README.md")),
            subagent_ended("toolu-1", "There's a README here.", 1_000),
            text("done", "done"),
        ],
    );
    let (world, _) = chat_of(events, 0);
    let (_hub, mut ui) = open(cx, world);
    let transcript = ui.transcript().unwrap();
    assert_eq!(ui.labelled(SUBAGENT_LINE).first().and_then(ElementSnapshot::expanded), Some(false));

    find(&mut ui, "around.");

    ui.wait("the match", |ui| says(ui).as_deref() == Some("1 of 1"));
    ui.wait("its call to open", |ui| {
        ui.labelled(SUBAGENT_LINE).first().and_then(ElementSnapshot::expanded) == Some(true)
    });
    // What it was asked is drawn from the kit's blocks, which say where they drew the match being shown.
    ui.wait("the match to be drawn with its marks", |ui| {
        ui.cx.read_entity(&transcript, |t, _| t.match_in_view()) == Some(true)
    });
}

/// The wheel over the bar scrolls the chat under it, as it does anywhere else in it.
#[gpui_kit::test]
fn the_wheel_over_the_bar_scrolls_the_chat(cx: &mut TestAppContext) {
    let mut events = Vec::new();
    messages(&mut events, 100);
    let (world, _) = chat_of(events, 0);
    let (_hub, mut ui) = open(cx, world);
    find(&mut ui, "zebra");
    ui.wait("the bar to say so", |ui| says(ui).is_some_and(|says| says.contains("No matches")));
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().top());
    let before = top(&ui);

    ui.scroll("find-says", ScrollDelta::Pixels(point(px(0.), px(300.))));
    ui.settle(Duration::from_millis(16));

    assert_ne!(top(&ui), before, "it scrolled");
}
