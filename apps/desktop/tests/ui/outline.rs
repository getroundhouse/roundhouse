//! The outline at the right of a chat: a tick for each prompt, and the list that opens over them.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, ElementId, Pixels, ScrollDelta, SharedString, TestAppContext, point, px};
use protocol::event::EventBody;
use protocol::model::{Seq, seq_index};

use crate::support::{Stack, Ui};

/// Longer than the list waits to open or close.
const LONGER_THAN_THE_LIST_WAITS: Duration = Duration::from_secs(1);

#[gpui_kit::test]
fn prompts_too_far_back_to_be_loaded_are_listed_and_jumped_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:the first turn");
    stack.run(&session, "say:the second turn");
    // Far more events than the transcript loads to start with, so the prompts before it aren't among them.
    let script: Vec<String> = (0..700).map(|i| format!("say:message {i}")).collect();
    stack.run(&session, &script.join("; "));
    stack.run(&session, "say:the last turn");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the outline", |ui| ui.has("outline"));
    let listed = stack.client().session_prompt_list(&session).unwrap();
    let rows: Vec<usize> = listed.iter().map(|line| seq_index(line.seq()) - 1).collect();
    let (first, second) = (rows[0], rows[1]);
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(second).is_none()), "the second prompt isn't loaded");

    jump_to(&mut ui, second);

    ui.wait("the second prompt to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(second).is_some()));
    ui.frame();
    assert_eq!(scrolled_to(&ui), second, "the second prompt is at the top");
    assert!(selected(&mut ui, row_id("outline-tick", second)), "and its tick is lit");
    assert!(!ui.has("outline-list"), "the list closes, so it isn't over where it jumped to");

    jump_to(&mut ui, first);
    ui.frame();
    assert_eq!(scrolled_to(&ui), first, "and so is the first, from there");
}

#[gpui_kit::test]
fn from_the_end_of_a_chat_any_prompt_with_a_screen_below_it_is_jumped_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // Long replies, so every prompt but the last few has more than a screen of chat below it.
    for turn in 0..12 {
        stack.run(&session, &format!("prose:4:{turn}"));
    }
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let rows = prompt_rows(&ui);
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > rows[10]);

    for prompt in [2, 7, 4, 0, 9] {
        jump_to(&mut ui, rows[prompt]);
        assert_eq!(scrolled_to(&ui), rows[prompt], "prompt {prompt} is at the top");
        assert!(selected(&mut ui, row_id("outline-tick", rows[prompt])), "and its tick is lit");
    }
}

#[gpui_kit::test]
fn a_prompt_on_the_last_screen_cant_come_to_the_top_so_the_chat_stays_at_its_end(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    for turn in 0..6 {
        stack.run(&session, &format!("prose:4:{turn}"));
    }
    // Short, so the last three prompts are all on the last screen.
    for turn in 0..3 {
        stack.run(&session, &format!("say:short turn {turn}"));
    }
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let rows = prompt_rows(&ui);
    let (next_to_last, last) = (rows[rows.len() - 2], rows[rows.len() - 1]);
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > rows[3]);
    let end = scrolled_to(&ui);
    assert!(end < next_to_last, "the prompt shows already");

    jump_to(&mut ui, next_to_last);

    assert_eq!(scrolled_to(&ui), end, "the chat stays at its end");
    assert!(selected(&mut ui, row_id("outline-tick", last)), "where the last prompt is the one being read");
    assert!(!ui.has("outline-list"));
}

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

    assert_eq!(scrolled_to(&ui), first, "the chat scrolls to its start");
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
    let before = bounds(&mut ui, "outline-list");

    stack.run(&session, "say:third");
    ui.wait("the third prompt", |ui| prompt_rows(ui).len() == 3);
    let third = row_id("outline-prompt", prompt_rows(&ui)[2]);
    ui.wait("it to join the list", |ui| ui.has(third.clone()));

    assert!(ui.visible(third), "it shows");
    let after = bounds(&mut ui, "outline-list");
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
    let card = bounds(&mut ui, "outline-list");
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
    ui.hover("new-chat");
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
    let (list, ticks) = (bounds(&mut ui, "outline-list"), bounds(&mut ui, "outline"));
    let covers = list.left() <= ticks.left()
        && list.top() <= ticks.top()
        && list.right() >= ticks.right()
        && list.bottom() >= ticks.bottom();
    assert!(covers, "and hides the ticks: {list:?} {ticks:?}");
    assert!(list.size.height < px(150.), "as tall as its two rows: {list:?}");
}

#[gpui_kit::test]
fn after_a_jump_the_list_opens_again_only_once_the_pointer_moves_over_the_ticks(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let first = prompt_rows(&ui)[0];
    open_the_list(&mut ui);

    // Where the row covers the ticks, so once the list has gone the pointer rests on them.
    let (row, ticks) = (bounds(&mut ui, row_id("outline-prompt", first)), bounds(&mut ui, "outline"));
    let on_the_ticks = point(ticks.origin.x + px(2.), ticks.origin.y + px(8.));
    assert!(row.contains(&on_the_ticks), "the row covers the ticks there: {row:?} {ticks:?}");
    let offset = on_the_ticks - row.origin;
    let target = row_id("outline-prompt", first);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_at(target, offset, cx)).unwrap();
    ui.frame();
    for _ in 0..3 {
        wait_out_the_list(&mut ui);
    }
    assert!(!ui.has("outline-list"), "the list stays closed, though the pointer is on the ticks");

    open_the_list(&mut ui);
    assert!(ui.has("outline-list"), "until it moves over them");
}

#[gpui_kit::test]
fn a_list_left_open_in_a_chat_that_stopped_showing_is_closed_when_it_shows_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:first");
    stack.run(&session, "say:second");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    ui.press("cmd-t");
    ui.wait("a second chat", |ui| !ui.has("outline"));
    let other = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.session().unwrap().id.clone());
    ui.activate_chat(&session);
    ui.wait("the first chat", |ui| ui.has("outline"));
    open_the_list(&mut ui);

    // Without the pointer or a key, as when another client closes the chat, so the list never sees the pointer go.
    ui.activate_chat(&other);
    ui.wait("the second chat", |ui| !ui.has("outline"));
    ui.hover("new-chat");
    ui.activate_chat(&session);
    ui.wait("the first chat again", |ui| ui.has("outline"));
    wait_out_the_list(&mut ui);

    assert!(!ui.has("outline-list"));
}

#[gpui_kit::test]
fn a_chat_with_one_long_turn_jumps_back_up_to_its_prompt(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "burst:3000; say:only");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let [only] = prompt_rows(&ui)[..] else { panic!("one prompt: {:?}", prompt_rows(&ui)) };
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > only);

    jump_to(&mut ui, only);

    assert_eq!(scrolled_to(&ui), only, "the prompt is at the top");
    assert!(selected(&mut ui, row_id("outline-tick", only)), "and its tick is lit");
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
    let (ticks, tick) = (bounds(&mut ui, "outline"), bounds(&mut ui, only));
    assert!(ticks.size.height >= tick.size.height * 3., "three ticks tall: {ticks:?} {tick:?}");
    assert!(!tick.contains(&ticks.center()), "so the middle, where the pointer goes, is past the tick: {ticks:?}");

    open_the_list(&mut ui);

    assert!(ui.has("outline-list"));
}

fn open_the_list(ui: &mut Ui<'_>) {
    ui.hover("outline");
    wait_out_the_list(ui);
    ui.wait("the list", |ui| ui.has("outline-list"));
}

/// Opens the list and clicks the prompt drawn in `row`, turning the wheel over the list first if it doesn't show it,
/// as someone would.
fn jump_to(ui: &mut Ui<'_>, row: usize) {
    open_the_list(ui);
    let target = row_id("outline-prompt", row);
    for up in [true, false] {
        for _ in 0..40 {
            if ui.visible(target.clone()) {
                break;
            }
            let by = if up { px(60.) } else { px(-60.) };
            ui.scroll("outline-list", ScrollDelta::Pixels(point(px(0.), by)));
        }
    }
    ui.click(target);
    ui.frame();
}

fn bounds(ui: &mut Ui<'_>, id: impl Into<ElementId>) -> Bounds<Pixels> {
    let id = id.into();
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
}

/// Moves the app's clock past when the list would open or close.
fn wait_out_the_list(ui: &mut Ui<'_>) {
    ui.cx.executor().advance_clock(LONGER_THAN_THE_LIST_WAITS);
    ui.frame();
}

/// The rows the chat's prompts are drawn in, as the hub recorded them.
fn prompt_rows(ui: &Ui<'_>) -> Vec<usize> {
    let seqs: Vec<Seq> =
        ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).map(|e| e.seq).collect();
    seqs.into_iter().map(|seq| seq_index(seq) - 1).collect()
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
