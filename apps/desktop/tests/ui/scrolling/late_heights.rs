//! Scrolling through rows whose height comes late, such as a long text parsed on another thread, which moves
//! nothing in the window when it does.

use gpui_kit::test::{ElementSnapshot, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, Entity, Pixels, ScrollDelta, TestAppContext, point, px};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::SessionStatus;
use transcript::{Place, Transcript};

use super::{A_FRAME, settle_at_the_end};
use crate::support::{Stack, Ui};

/// How far each turn of the wheel goes, about what a trackpad sends at once.
const STEP: Pixels = px(40.);

/// A long thought, shown open as "Always expand thinking" has it, just above a long reply at the end of a session.
/// A text over 4 KB is parsed on another thread and has no height until that's done. A thought was only
/// parsed once it scrolled in at the top of the window, as a line with nothing under it, and then it grew there by all
/// of its length, which took the window up to the thought's start, past the reply being scrolled up through.
#[gpui_kit::test]
fn scrolling_up_into_a_long_open_thought_moves_as_far_as_the_wheel_turns(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:a short reply first");
    stack.run(&session, "ponder:40:1; prose:40:2");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    settle_at_the_end(&mut ui, &stack, &session);

    let steps = steps_up_to_the_start(&mut ui, &transcript);

    each_went_as_far_as_the_wheel(&steps);
}

/// A long thought that's been closed has its height at once when it's opened again, rather than being parsed again on
/// another thread, and drawn as a line with nothing under it meanwhile.
#[gpui_kit::test]
fn a_long_thought_opened_again_has_its_height_at_once(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:a short reply first");
    stack.run(&session, "ponder:40:1; say:the end");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let thought = thought_row(&ui, &transcript);
    let height = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().row_height(thought));
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    settle_at_the_end(&mut ui, &stack, &session);
    let opened = height(&ui);
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(false, cx));
    settle_at_the_end(&mut ui, &stack, &session);

    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    // One frame, with nothing else let run, so a text parsed on another thread wouldn't be in yet.
    ui.cx.update_window(ui.handle, |_, window, cx| window.render_frame(cx)).unwrap();

    assert_eq!(height(&ui), opened, "it's as tall as it was");
    assert!(ui.cx.read_entity(&transcript, |t, _| t.long_texts_parsed()), "with nothing parsed again");
}

/// A long plan, approved, just above a long reply. A plan is drawn in an approval's card, a row unlike a reply's, so
/// this shows that whatever long text a row draws is kept.
#[gpui_kit::test]
fn scrolling_up_into_a_long_plan_moves_as_far_as_the_wheel_turns(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:a short reply first");
    let plan = vec!["the signal box keeps a ledger of every train that passes. ".repeat(12); 12].join("\n\n");
    assert!(plan.len() > 4 * 1024, "a plan parsed on another thread");
    stack.client().session_prompt(&session, &format!("plan:{plan}"), None).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let approval = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("approve-plan".into(), approval.get()));
    ui.wait("the plan's turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    stack.run(&session, "prose:40:2");
    let transcript = ui.transcript().unwrap();
    settle_at_the_end(&mut ui, &stack, &session);

    let steps = steps_up_to_the_start(&mut ui, &transcript);

    each_went_as_far_as_the_wheel(&steps);
}

/// A long thought whose parse lands once it's at the top of the window, as when it takes longer to parse than
/// someone takes to scroll to it, or a fling or a drag of the scrollbar's thumb takes the window to it before its row
/// has been drawn. It's drawn then as a line with nothing under it, and when it's parsed it grows by all of its length.
/// What's on screen stays where it is.
#[gpui_kit::test]
fn a_long_text_parsed_once_it_is_at_the_top_of_the_window_moves_nothing_on_screen(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:a short reply first");
    stack.run(&session, "ponder:40:1; prose:40:2");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    // Closed, so the thought's text isn't drawn, and nothing has parsed it.
    settle_at_the_end(&mut ui, &stack, &session);
    let thought = thought_row(&ui, &transcript);
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().top());

    // Up through the reply a little at a time, each frame drawn with nothing else let run, so the thought's parse
    // is still under way when it comes to the top of the window.
    while top(&ui).row > thought {
        let up = ScrollDelta::Pixels(point(px(0.), px(10.)));
        ui.cx.update_window(ui.handle, |_, window, cx| window.scroll("transcript", up, cx)).unwrap();
    }
    assert_eq!(top(&ui).row, thought, "the thought is at the top, as a line");
    assert!(!ui.cx.read_entity(&transcript, |t, _| t.long_texts_parsed()), "and not parsed yet");
    let before = to_the_end(&ui, &transcript);
    ui.settle(A_FRAME);

    let after = to_the_end(&ui, &transcript);
    assert!((after - before).abs() < px(0.5), "what's on screen moved {:?} as the thought was parsed", after - before);
}

/// A long thought, closed, opened from its line while the line is cut off at the top of the window. The line stays
/// where it was clicked, and the thought opens under it, as it does once it's parsed, rather than growing up past the
/// window's top and taking the line with it.
#[gpui_kit::test]
fn a_long_thought_opened_from_its_line_at_the_top_of_the_window_opens_under_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:a short reply first");
    stack.run(&session, "ponder:40:1; prose:40:2");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    settle_at_the_end(&mut ui, &stack, &session);
    let thought = thought_row(&ui, &transcript);
    while ui.cx.read_entity(&transcript, |t, _| t.list().top().row) > thought {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), STEP)));
        ui.settle(A_FRAME);
    }
    // Then until only the bottom of the line shows.
    let above = |ui: &mut Ui<'_>| window_top(ui) - line(ui).bounds().top();
    let turn = above(&mut ui) - CUT;
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), turn)));
    ui.settle(A_FRAME);
    assert!((above(&mut ui) - CUT).abs() < px(0.5), "the line is cut off at the window's top");
    let before = line(&mut ui).bounds().top();

    let id = line(&mut ui).path().last().unwrap().clone();
    let at = point(px(20.), CUT + px(4.));
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_at(id, at, cx)).unwrap();
    ui.settle(A_FRAME);

    assert_eq!(line(&mut ui).expanded(), Some(true), "the thought is open");
    assert!(ui.cx.read_entity(&transcript, |t, _| t.long_texts_parsed()), "and parsed");
    let moved = line(&mut ui).bounds().top() - before;
    assert!(moved.abs() < px(0.5), "the line moved {moved:?} as the thought opened under it");
}

/// How much of the thought's line is above the window's top in the test of opening it there.
const CUT: Pixels = px(8.);

/// The one thought's line.
fn line(ui: &mut Ui<'_>) -> ElementSnapshot {
    let mut lines = ui.labelled("Thinking");
    assert_eq!(lines.len(), 1, "one thought's line");
    lines.remove(0)
}

/// Where the top of the window the transcript's rows show in is.
fn window_top(ui: &mut Ui<'_>) -> Pixels {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds().top()).unwrap()
}

/// The row of the session's one thought.
fn thought_row(ui: &Ui<'_>, transcript: &Entity<Transcript>) -> usize {
    ui.cx.read_entity(transcript, |t, _| {
        (0..t.row_count()).find(|row| t.event_at(*row).is_some_and(|event| thinks(&event.body))).unwrap()
    })
}

/// Whether an event holds a thought.
fn thinks(body: &EventBody) -> bool {
    let EventBody::Assistant { blocks, .. } = body else { return false };
    blocks.iter().any(|block| matches!(block, ContentBlock::Thinking { .. }))
}

/// How far it is from the top of the window to the end of the session, by the rows' heights as measured.
fn to_the_end(ui: &Ui<'_>, transcript: &Entity<Transcript>) -> Pixels {
    ui.cx.read_entity(transcript, |t, _| {
        let (list, top) = (t.list(), t.list().top());
        let below: Pixels = (top.row..list.row_count()).map(|row| list.row_height(row).unwrap_or_default()).sum();
        below - top.offset
    })
}

/// Turns the wheel up a [`STEP`] at a time, as a trackpad does, until the list is at the start, letting the list come
/// to rest after each. Returns how far it was from the top of the window to the end of the session after each, by
/// the rows' heights as measured.
fn steps_up_to_the_start(ui: &mut Ui<'_>, transcript: &Entity<Transcript>) -> Vec<Pixels> {
    let top = |ui: &Ui<'_>| ui.cx.read_entity(transcript, |t, _| t.list().top());
    let mut steps = vec![to_the_end(ui, transcript)];
    while top(ui) != Place::default() {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), STEP)));
        ui.settle(A_FRAME);
        steps.push(to_the_end(ui, transcript));
        assert!(steps.len() < 2000, "the top is never reached");
    }
    steps
}

/// Every step went as far as the wheel turned, but the last, which stops at the start however far that is.
fn each_went_as_far_as_the_wheel(steps: &[Pixels]) {
    let moved: Vec<Pixels> = steps.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let (last, before) = moved.split_last().unwrap();
    assert!(*last <= STEP + px(0.5), "the last step went {last:?}");
    let off = before.iter().enumerate().find(|(_, moved)| (**moved - STEP).abs() > px(0.5));
    assert!(off.is_none(), "step {off:?} of {} went another way than the wheel's {STEP:?}", before.len());
}
