//! Selecting what a call shows of its own, in a row of its own: what the advisor said, and what a subagent was asked
//! and reported, with the rest while the call is open, in the order the chat shows it, and a long one's area scrolling
//! on under a drag from it, for advice and for what a subagent was asked.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, ElementId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    TestAppContext, point, px,
};

use super::super::{A_FRAME, press, wait_for_the_list_to_settle};
use super::{LAST, bounds, everything, on_the_reply_saying, open};
use crate::support::{Stack, Ui};

const ADVISOR_LINE: &str = "Ask the advisor";
/// What the advisor says, in words nothing else in the chat says as its own text.
const ADVICE: &str = "Test the points before the timetable changes.";
/// The fake agent's subagent's call, and what it's asked, which the prompt that starts it doesn't say with the stop.
pub(super) const SUBAGENT_LINE: &str = "Look around";
pub(super) const ASKED: &str = "Look around.";
/// What it reports, once it's done.
pub(super) const REPORT: &str = "There's a README here.";

/// What the advisor said is copied with the rest while its call is open, between what came before and after it, and
/// isn't once it's closed.
#[gpui_kit::test]
fn what_the_advisor_said_is_selected_with_the_rest_only_while_its_call_is_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("advise:{ADVICE}; say:{LAST}"));
    let mut ui = open(cx, &stack);
    settled(&mut ui);

    let opened = everything_with(&mut ui, ADVISOR_LINE);
    let closed = everything_with(&mut ui, ADVISOR_LINE);

    // The prompt says it too, before what follows it there.
    assert!(opened.contains(&format!("\n\n{ADVICE}\n\n{LAST}")), "opened, between the rest: {opened:?}");
    assert_eq!(closed.matches(ADVICE).count(), 1, "closed, only the prompt says it: {closed:?}");
}

/// What a subagent was asked, and its report, are copied with the rest while its call is open, in that order, after the
/// prompt that started it, and aren't once it's closed.
#[gpui_kit::test]
fn what_a_subagent_was_asked_and_reported_is_selected_with_the_rest_only_while_its_call_is_open(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("agent:{SUBAGENT_LINE}; say:{LAST}"));
    let mut ui = open(cx, &stack);
    settled(&mut ui);

    let opened = everything_with(&mut ui, SUBAGENT_LINE);
    let closed = everything_with(&mut ui, SUBAGENT_LINE);

    assert!(
        opened.contains(&format!("{SUBAGENT_LINE}; say:{LAST}\n\n{ASKED}\n\n{REPORT}\n\n{LAST}")),
        "opened, after the prompt: {opened:?}"
    );
    assert!(!closed.contains(ASKED) && !closed.contains(REPORT), "closed, they aren't: {closed:?}");
}

/// A drag that starts in what the advisor said, held below the area that scrolls it, scrolls it on to its end, as
/// the wheel would, and rests there.
#[gpui_kit::test]
fn a_drag_held_past_long_advice_scrolls_its_area_on(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("advise:{}; say:{LAST}", long()));
    let mut ui = open(cx, &stack);

    held_past_its_area(&mut ui, ADVISOR_LINE, "advice/");
}

/// So does one that starts in what a subagent was asked, at length.
#[gpui_kit::test]
fn a_drag_held_past_a_long_ask_of_a_subagent_scrolls_its_area_on(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // It's asked what it's for, which its line says.
    let line = long();
    stack.run(&session, &format!("agent:{line}; say:{LAST}"));
    let mut ui = open(cx, &stack);

    // What it was asked, rather than the prompt that started it, is known by its call's place in the chat.
    held_past_its_area(&mut ui, &line, "prompt/event/");
}

/// Words enough to be scrolled far past the area they're in, that nothing else in the chat says.
pub(super) fn long() -> String {
    format!("{}Junctions last", "Timetables, sidings and points. ".repeat(120))
}

/// Opens the call whose line says `line`, and holds a drag from its text whose ID starts with `part`, as [`held_past`]
/// does.
fn held_past_its_area(ui: &mut Ui<'_>, line: &str, part: &str) {
    settled(ui);
    ui.click_labelled(line, 0);
    settled(ui);
    held_past(ui, part);
}

/// Presses in the text whose ID starts with `part`, and holds a drag below the area that scrolls it, which the area
/// should follow to the text's end.
pub(super) fn held_past(ui: &mut Ui<'_>, part: &str) {
    let text = ui.shown(part).into_iter().next().expect("the text, drawn");
    let area = bounds(ui, ElementId::from((ElementId::from((text.clone(), "scroll")), "area")));
    let before = bounds(ui, text.clone());
    let start = point(area.left() + px(60.), area.top() + px(20.));
    let below = point(start.x, area.bottom() + px(30.));

    let (button, held) = (MouseButton::Left, Some(MouseButton::Left));
    press(ui, MouseDownEvent { button, position: start, click_count: 1, ..Default::default() });
    press(ui, MouseMoveEvent { position: below, pressed_button: held, ..Default::default() });
    ui.settle(A_FRAME);
    let after = bounds(ui, text);
    press(ui, MouseUpEvent { button, position: below, click_count: 1, ..Default::default() });

    assert!(before.bottom() > area.bottom() + px(100.), "it starts far past its area: {before:?} in {area:?}");
    assert!((after.bottom() - area.bottom()).abs() < px(2.), "and ends at its end: {after:?} in {area:?}");
}

/// Everything Cmd-A selects once the call whose line says `line` has been opened, or closed if it was open, with the
/// chat given the focus by a click on the last reply.
fn everything_with(ui: &mut Ui<'_>, line: &str) -> String {
    ui.click_labelled(line, 0);
    let window = settled(ui);
    let on_it = on_the_reply_saying(ui, window, LAST);
    everything(ui, on_it)
}

/// Where the transcript is, once its list has settled at the chat's last row. A subagent's events have places in the
/// log the chat's rows don't, so this goes by the rows.
fn settled(ui: &mut Ui<'_>) -> Bounds<Pixels> {
    let rows = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.row_count());
    wait_for_the_list_to_settle(ui, rows - 1);
    ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap()
}
