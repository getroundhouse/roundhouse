//! Jumping to a prompt from the outline's list, which puts it at the bottom of the chat, with what it answered above.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Pixels, ScrollDelta, TestAppContext, point, px};
use protocol::model::seq_index;

use super::{
    compact, open_the_list, prompt_rows, row_id, scrolled_to, scrolled_to_exactly, selected, wait_out_the_list,
};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn prompts_too_far_back_to_be_loaded_are_listed_and_jumped_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // Long enough for the second prompt to come to the bottom.
    stack.run(&session, "prose:4:0");
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
    // Each is drawn where the agent took it, which the hub lists with it.
    let rows: Vec<usize> = listed.iter().map(|line| seq_index(line.settled().unwrap().seq) - 1).collect();
    let (first, second) = (rows[0], rows[1]);
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(second).is_none()), "the second prompt isn't loaded");

    jump_to(&mut ui, second);

    ui.wait("the second prompt to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(second).is_some()));
    assert_at_the_bottom(&mut ui, second, "the second prompt");
    assert!(selected(&mut ui, row_id("outline-tick", second)), "and its tick is lit");
    assert!(!ui.has("outline-list"), "the list closes, so it isn't over where it jumped to");

    jump_to(&mut ui, first);
    ui.frame();
    assert_eq!(scrolled_to_exactly(&ui), (0, px(0.)), "and the first, from there, is at the start");
}

#[gpui_kit::test]
fn from_the_end_of_a_chat_any_prompt_with_a_screen_above_it_comes_to_the_bottom(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // Long replies, so every prompt but the first few has more than a screen of chat above it.
    for turn in 0..12 {
        stack.run(&session, &format!("prose:4:{turn}"));
    }
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let rows = prompt_rows(&ui);
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > rows[10]);

    // The last too, with its reply below it, and from the end of the chat.
    for prompt in [11, 2, 7, 4, 9] {
        jump_to(&mut ui, rows[prompt]);
        assert_at_the_bottom(&mut ui, rows[prompt], &format!("prompt {prompt}"));
        assert!(selected(&mut ui, row_id("outline-tick", rows[prompt])), "and its tick is lit");
    }
}

#[gpui_kit::test]
fn a_prompt_on_the_first_screen_cant_come_to_the_bottom_so_the_chat_goes_to_its_start(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // Short, so the first three prompts are all on the first screen.
    for turn in 0..3 {
        stack.run(&session, &format!("say:short turn {turn}"));
    }
    for turn in 0..6 {
        stack.run(&session, &format!("prose:4:{turn}"));
    }
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the outline", |ui| ui.has("outline"));
    let rows = prompt_rows(&ui);
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > rows[3]);

    jump_to(&mut ui, rows[1]);

    assert_eq!(scrolled_to_exactly(&ui), (0, px(0.)), "the chat is at its start");
    assert!(last_prompt_in_sight(&mut ui, &rows) > 1, "where the prompt shows already, with more after it");
    assert!(selected(&mut ui, row_id("outline-tick", rows[1])), "and it's the one being read");
    assert!(!ui.has("outline-list"));
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(-10.))));
    ui.bring_the_next_frame();
    let in_sight = last_prompt_in_sight(&mut ui, &rows);
    assert!(selected(&mut ui, row_id("outline-tick", rows[in_sight])), "until the chat moves, then the last in sight");
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

    // The rows before it, where the agent started, draw nothing.
    assert_eq!(
        scrolled_to_exactly(&ui),
        (0, px(0.)),
        "the chat is at its start, since the prompt can't reach the bottom"
    );
    assert!(selected(&mut ui, row_id("outline-tick", only)), "and its tick is lit");
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
    let (row, ticks) = (ui.bounds(row_id("outline-prompt", first)), ui.bounds("outline"));
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
fn a_compaction_too_far_back_to_be_loaded_is_marked_and_jumped_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // Long replies, so the compaction after them has more than a screen of chat above it and can come to the bottom.
    for turn in 0..3 {
        stack.run(&session, &format!("prose:4:{turn}"));
    }
    compact(&stack, &session);
    // Far more events than the transcript loads to start with, so the compaction before them isn't among them.
    let script: Vec<String> = (0..700).map(|i| format!("say:message {i}")).collect();
    stack.run(&session, &script.join("; "));
    stack.run(&session, "say:the last turn");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let listed = stack.client().session_compaction_list(&session).unwrap();
    let [ref compaction] = listed[..] else { panic!("one compaction: {listed:?}") };
    let row = seq_index(compaction.seq) - 1;
    ui.wait("its dot", |ui| ui.has(row_id("outline-dot", row)));
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_none()), "the compaction isn't loaded");

    jump_to_line(&mut ui, row_id("outline-compaction", row));

    ui.wait("the compaction to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_some()));
    ui.frame();
    assert_at_the_bottom(&mut ui, row, "the compaction");
    let before = prompt_rows(&ui)[2];
    assert!(selected(&mut ui, row_id("outline-tick", before)), "and the prompt before it is the one being read");
    assert!(!selected(&mut ui, row_id("outline-dot", row)), "since a compaction never is");
}

/// Opens the list and clicks the prompt drawn in `row`, turning the wheel over the list first if it doesn't show it,
/// as someone would.
fn jump_to(ui: &mut Ui<'_>, row: usize) {
    jump_to_line(ui, row_id("outline-prompt", row));
}

/// Opens the list and clicks its row `target`, a prompt's or a compaction's, turning the wheel over the list first if
/// it doesn't show it.
fn jump_to_line(ui: &mut Ui<'_>, target: ElementId) {
    open_the_list(ui);
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
    // The frame after the click draws what laying the rows out found, such as the chat's end in sight.
    ui.frame();
    ui.bring_the_next_frame();
    ui.frame();
}

/// Asserts that the end of the row drawn in `row` is at the bottom of the chat, to within a pixel. It waits first for
/// every row from the top of the chat down to it to have been laid out, over as many frames as that takes, since a row
/// jumped to can load before the rows above it, which can be in a page that comes after its own, and a row that hasn't
/// come isn't measured.
fn assert_at_the_bottom(ui: &mut Ui<'_>, row: usize, what: &str) {
    ui.wait("the rows down to it to be laid out", |ui| laid_out_down_to(ui, row));
    let (end, tall) = (place_of(ui, row).1, ui.bounds("transcript").size.height);
    assert!((end - tall).abs() < px(1.), "{what} ends {end:?} down a chat {tall:?} tall");
}

/// The prompt, of those drawn in `rows`, that's the last with more than a pixel of it in the chat, found from the rows
/// in the window alone, as the last frame measured them.
fn last_prompt_in_sight(ui: &mut Ui<'_>, rows: &[usize]) -> usize {
    let tall = ui.bounds("transcript").size.height - px(1.);
    let (mut row, offset) = scrolled_to_exactly(ui);
    let mut down = -offset;
    while down < tall {
        down += row_height(ui, row);
        row += 1;
    }
    // Now the first row that starts below that.
    rows.iter().rposition(|prompt| *prompt < row).unwrap()
}

/// How far below the top of the chat the row drawn in `row` starts and ends, as the last frame measured the rows.
fn place_of(ui: &Ui<'_>, row: usize) -> (Pixels, Pixels) {
    let (top, offset) = scrolled_to_exactly(ui);
    let start = (top..row).map(|row| row_height(ui, row)).fold(-offset, |down, height| down + height);
    (start, start + row_height(ui, row))
}

/// Whether the last frame measured every row from the top of the chat down to the one drawn in `row`.
fn laid_out_down_to(ui: &Ui<'_>, row: usize) -> bool {
    (scrolled_to(ui)..=row).all(|row| measured(ui, row).is_some())
}

fn row_height(ui: &Ui<'_>, row: usize) -> Pixels {
    measured(ui, row).unwrap()
}

/// How tall the row drawn in `row` was when it was last measured, if it has been.
fn measured(ui: &Ui<'_>, row: usize) -> Option<Pixels> {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().row_height(row))
}
