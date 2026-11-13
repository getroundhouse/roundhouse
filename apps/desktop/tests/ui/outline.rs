//! The outline at the right of a chat: a tick for each prompt and a dot for each compaction, and the list that opens
//! over them.

#![cfg(test)]

use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Pixels, ScrollDelta, SharedString, TestAppContext, point, px};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{Seq, SessionId, SessionStatus, SettledAt, seq_index};

use crate::support::{Stack, TIMEOUT, Ui};

mod jumps;

/// Longer than the list waits to open or close.
const LONGER_THAN_THE_LIST_WAITS: Duration = Duration::from_secs(1);

#[gpui_kit::test]
fn the_wheel_over_the_ticks_scrolls_the_chat_and_the_lit_tick_follows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "burst:3000; say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let [first, second] = prompt_rows(&ui)[..] else { panic!("two prompts: {:?}", prompt_rows(&ui)) };
    ui.wait("the end of the chat", |ui| ui.transcript().is_some_and(|_| scrolled_to(ui) > first));
    assert!(selected(&mut ui, row_id("outline-tick", second)), "at the end, the last prompt is lit");

    ui.scroll("outline", ScrollDelta::Pixels(point(px(0.), px(100_000.))));
    // The ticks go by the rows as the last frame laid them out, so they're right in the frame after.
    ui.bring_the_next_frame();

    assert_eq!(scrolled_to(&ui), 0, "the chat scrolls to its start");
    assert!(selected(&mut ui, row_id("outline-tick", first)), "and the first prompt is lit");
    assert!(!selected(&mut ui, row_id("outline-tick", second)));
}

/// A trackpad's wheel doesn't move the pointer, so a pointer on the ticks while it turns rests there as long as it
/// turns, which isn't asking for the list.
#[gpui_kit::test]
fn the_wheel_over_the_ticks_goes_on_scrolling_the_chat_rather_than_opening_the_list(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "burst:3000; say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let first = prompt_rows(&ui)[0];
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > first);
    let end = scrolled_to_exactly(&ui);
    let up = ScrollDelta::Pixels(point(px(0.), px(300.)));

    ui.scroll("outline", up);
    wait_out_the_list(&mut ui);
    let partway = scrolled_to_exactly(&ui);
    ui.scroll("outline", up);
    wait_out_the_list(&mut ui);

    assert!(!ui.has("outline-list"), "the list stays closed");
    let now = scrolled_to_exactly(&ui);
    assert!(now < partway && partway < end, "and the wheel goes on scrolling the chat: {end:?} {partway:?} {now:?}");
    open_the_list(&mut ui);
    assert!(ui.has("outline-list"), "until the pointer moves over the ticks");
}

#[gpui_kit::test]
fn the_wheel_over_the_list_scrolls_the_list_and_not_the_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // More than the list has room for, and long enough that the chat can move.
    for turn in 0..16 {
        stack.run(&session, &format!("prose:2:{turn}"));
    }
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let first = row_id("outline-prompt", prompt_rows(&ui)[0]);
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > 0);
    open_the_list(&mut ui);
    let chat = scrolled_to(&ui);
    assert!(!ui.visible(first.clone()), "the list opens at its end");

    ui.scroll("outline-list", ScrollDelta::Pixels(point(px(0.), px(100_000.))));

    assert!(ui.visible(first), "the list scrolls to its start");
    assert_eq!(scrolled_to(&ui), chat, "and the chat under it stays where it was");
}

#[gpui_kit::test]
fn a_prompt_recorded_while_the_list_is_open_joins_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    open_the_list(&mut ui);
    let before = ui.bounds("outline-list");

    stack.run(&session, "say:third");
    ui.wait("the third prompt", |ui| prompt_rows(ui).len() == 3);
    let third = row_id("outline-prompt", prompt_rows(&ui)[2]);
    ui.wait("it to join the list", |ui| ui.has(third.clone()));

    assert!(ui.visible(third), "it shows");
    let after = ui.bounds("outline-list");
    assert!(after.size.height > before.size.height, "and the list grows to fit it: {before:?} {after:?}");
}

#[gpui_kit::test]
fn the_list_opens_scrolled_to_the_prompt_being_read(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // More than the list has room for.
    for turn in 0..16 {
        stack.run(&session, &format!("say:turn {turn}"));
    }
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let rows = prompt_rows(&ui);
    let (first, last) = (rows[0], rows[rows.len() - 1]);

    open_the_list(&mut ui);

    assert!(selected(&mut ui, row_id("outline-prompt", last)), "at the end, the last prompt is the one being read");
    assert!(ui.visible(row_id("outline-prompt", last)), "and the list shows it");
    assert!(!ui.visible(row_id("outline-prompt", first)), "rather than its start");
    let card = ui.bounds("outline-list");
    assert!(card.size.height <= px(440.), "stopping short of all sixteen rows: {card:?}");
}

#[gpui_kit::test]
fn the_list_stays_while_the_pointer_is_on_it_and_goes_once_it_leaves(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    open_the_list(&mut ui);

    ui.hover("outline-list");
    wait_out_the_list(&mut ui);

    assert!(ui.has("outline-list"), "the pointer moved onto the list");
    ui.hover("new-tab");
    wait_out_the_list(&mut ui);
    assert!(!ui.has("outline-list"), "and once it's left, the list goes");
}

#[gpui_kit::test]
fn the_list_opens_over_the_ticks_and_stays_while_the_pointer_rests_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));

    open_the_list(&mut ui);
    for _ in 0..5 {
        wait_out_the_list(&mut ui);
    }

    assert!(ui.has("outline-list"), "the list, opening under the pointer, keeps it");
    let (list, ticks) = (ui.bounds("outline-list"), ui.bounds("outline"));
    let covers = list.left() <= ticks.left()
        && list.top() <= ticks.top()
        && list.right() >= ticks.right()
        && list.bottom() >= ticks.bottom();
    assert!(covers, "and hides the ticks: {list:?} {ticks:?}");
    assert!(list.size.height < px(150.), "as tall as its two rows: {list:?}");
}

#[gpui_kit::test]
fn a_list_left_open_in_a_chat_that_stopped_showing_is_closed_when_it_shows_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    ui.press("secondary-t");
    ui.wait("a second chat", |ui| !ui.has("outline"));
    let other = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.session().unwrap().id.clone());
    ui.activate_chat(&session);
    ui.wait("the first chat", |ui| ui.has("outline"));
    open_the_list(&mut ui);

    // Without the pointer or a key, as when another client closes the chat, so the list never sees the pointer go.
    ui.activate_chat(&other);
    ui.wait("the second chat", |ui| !ui.has("outline"));
    ui.hover("new-tab");
    ui.activate_chat(&session);
    ui.wait("the first chat again", |ui| ui.has("outline"));
    wait_out_the_list(&mut ui);

    assert!(!ui.has("outline-list"));
}

/// The list opens once the pointer moves over the ticks, so one tick alone would be a small place to find.
#[gpui_kit::test]
fn a_lone_tick_is_as_easy_to_rest_on_as_three(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:only");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let only = row_id("outline-tick", prompt_rows(&ui)[0]);
    let (ticks, tick) = (ui.bounds("outline"), ui.bounds(only));
    assert!(ticks.size.height >= tick.size.height * 3., "three ticks tall: {ticks:?} {tick:?}");
    assert!(!tick.contains(&ticks.center()), "so the middle, where the pointer goes, is past the tick: {ticks:?}");

    open_the_list(&mut ui);

    assert!(ui.has("outline-list"));
}

#[gpui_kit::test]
fn a_compaction_recorded_while_the_chat_shows_joins_its_outline(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:only");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));

    compact(&stack, &session);

    let compaction = stack.client().session_compaction_list(&session).unwrap()[0].clone();
    let row = seq_index(compaction.seq) - 1;
    ui.wait("its dot", |ui| ui.has(row_id("outline-dot", row)));
    let only = row_id("outline-tick", prompt_rows(&ui)[0]);
    let (tick, dot) = (ui.bounds(only), ui.bounds(row_id("outline-dot", row)));
    assert!(dot.top() >= tick.bottom(), "under the prompt it came after: {tick:?} {dot:?}");
}

/// Asks the hub to compact the chat now, and waits for the agent to have.
fn compact(stack: &Stack, session: &SessionId) {
    let client = stack.client();
    client.session_compact(session, None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session_compaction_list(session).unwrap().is_empty()
        || client.session_get(session).unwrap().status != SessionStatus::Idle
    {
        assert!(Instant::now() < deadline, "the agent never compacted");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn open_the_list(ui: &mut Ui<'_>) {
    ui.hover("outline");
    wait_out_the_list(ui);
    ui.wait("the list", |ui| ui.has("outline-list"));
}

/// Moves the app's clock past when the list would open or close.
fn wait_out_the_list(ui: &mut Ui<'_>) {
    ui.cx.executor().advance_clock(LONGER_THAN_THE_LIST_WAITS);
    ui.frame();
}

/// The row each prompt is drawn in: the row of the last event that took or dropped it, which is where the agent read
/// it, or its own while it's loaded with nothing to say so.
fn prompt_rows(ui: &Ui<'_>) -> Vec<usize> {
    let Some(transcript) = ui.transcript() else { return Vec::new() };
    let rows: Vec<(usize, SessionEvent)> = ui.cx.read_entity(&transcript, |t, _| {
        (0..t.row_count()).filter_map(|row| t.event_at(row).map(|e| (row, e))).collect()
    });
    let settled = |seq: Seq| {
        let settles = |e: &SessionEvent| SettledAt::of(e).is_some_and(|(prompt, _)| prompt == seq);
        rows.iter().rev().find(|(_, e)| settles(e)).map(|(row, _)| *row)
    };
    let prompts = rows.iter().filter(|(_, e)| matches!(e.body, EventBody::UserPrompt { .. }));
    prompts.map(|(row, e)| settled(e.seq).unwrap_or(*row)).collect()
}

fn row_id(part: &str, row: usize) -> ElementId {
    SharedString::from(format!("{part}/{row}")).into()
}

/// Whether it tells assistive apps it's the prompt being read.
fn selected(ui: &mut Ui<'_>, id: ElementId) -> bool {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).selected() == Some(true)).unwrap()
}

/// The row at the top of the chat.
fn scrolled_to(ui: &Ui<'_>) -> usize {
    scrolled_to_exactly(ui).0
}

/// The row at the top of the chat, and how far into it.
fn scrolled_to_exactly(ui: &Ui<'_>) -> (usize, Pixels) {
    let top = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().top());
    (top.row, top.offset)
}
