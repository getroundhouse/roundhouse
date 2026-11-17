//! Selecting in an area that scrolls a long text of its own within its row: a drag from it, held past the area, scrolls
//! the area on, for advice, for what a subagent was asked, for a command and for what it printed, and for a
//! compaction's summary, which scrolls back too, and a press on what the area has moved out of sight is on what's
//! drawn there instead. Only what the hub recorded matters, so each chat is made up for a fake hub.

use std::time::Duration;

use client_core::fake_hub::events::text;
use gpui_kit::{
    AppContext as _, Bounds, ElementId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollDelta, ScrollEvent, SharedString, TestAppContext, point, px,
};
use protocol::event::EventBody;

use super::super::{A_FRAME, press, turns};
use super::kinds::{ADVISOR_LINE, REPORT, settled_at_its_last_row};
use super::made_up::{advice, command, compacted, open_on_chats_of, show, subagent};
use super::{COMMAND_LINE, LAST, bounds, click, copy, head, on_the_reply_saying, tail};
use crate::support::{FakeHub, Ui};

/// What a long summary's list is made of, over and over, in words the reply doesn't have.
const KEPT: &str = "timetables sidings junctions ";
/// What follows the list, in words the reply doesn't have either, which starts out of sight.
const AFTER: &str = "Junctions, sidings, timetables.";
/// How the summary's list starts, out of sight once it's scrolled down.
const FIRST: &str = "What was asked";
/// Longer than a wheel's turns take to count as one gesture, so the next turn goes to what it's over.
const A_PAUSE: Duration = Duration::from_secs(1);

/// A drag that starts in a long text in an area of its own, held below the area, scrolls it on to its end, as the
/// wheel would, and rests there: in what the advisor said, what a subagent was asked, at length, a long command, and
/// what a command printed, at length. Each is in a chat of its own. The subagent tab's is in `super::tab`, since it
/// opens a tab of its own.
#[gpui_kit::test]
fn a_drag_held_past_each_long_text_in_a_row_s_area_scrolls_the_area_on(cx: &mut TestAppContext) {
    // A subagent's asked what it's for, which its line says, and what it was asked, rather than the prompt that started
    // it, is known by its call's place in the chat.
    let asked = long(1);
    // A command and what it printed are a line to each, of a height the font doesn't change much, so these are only
    // as many as take each past its area by twice what `held_past` asks, and no more, since the drag goes through them.
    let script = Vec::from_fn(20, |n| format!("echo signal {}", n + 1)).join("\n");
    let lines = |count: usize, line: &str| Vec::from_fn(count, |n| format!("{line}{}\n", n + 1)).concat();
    let areas = [
        ("long advice", advice(1, &long(1)), ADVISOR_LINE, "advice/"),
        (
            "a long ask of a subagent",
            subagent(1, &asked, &format!("{asked}."), REPORT),
            asked.as_str(),
            "prompt/event/",
        ),
        ("a long command", command(1, &script, &lines(20, "signal ")), COMMAND_LINE, "command/"),
        ("long output", command(1, "seq 1 18", &lines(18, "")), COMMAND_LINE, "output/"),
    ];
    let chat = |did: &Vec<EventBody>| {
        let mut did = did.clone();
        did.push(text("msg-last", LAST));
        turns(vec![("Look over the signals.".to_owned(), did)])
    };
    let (_fake, mut ui, chats, _) = open_on_chats_of(cx, Vec::from_iter(areas.iter().map(|(_, did, ..)| chat(did))));

    for ((name, _, line, part), chat) in areas.into_iter().zip(&chats) {
        show(&mut ui, chat);
        let lines = ui.labelled(line);
        let newest = lines.last().unwrap_or_else(|| panic!("{name}: no line labelled {line}"));
        ui.click(newest.path().last().unwrap().clone());
        ui.keep_to_the_end();
        settled_at_its_last_row(&mut ui);

        held_past(&mut ui, name, part);
    }
}

/// Words enough to be scrolled far past the area they're in, that nothing else in the chat says, `times` as many as
/// the fewest that are, since a drag that holds on takes longer the further it has to go.
pub(super) fn long(times: usize) -> String {
    format!("{}Junctions last", "Timetables, sidings and points. ".repeat(50 * times))
}

/// Presses in the newest text whose ID starts with `part`, and holds a drag below the area that scrolls it, which the
/// area should follow to the text's end. `name` says which text it is if it doesn't.
pub(super) fn held_past(ui: &mut Ui<'_>, name: &str, part: &str) {
    let text = ui.shown(part).pop().unwrap_or_else(|| panic!("{name}: the text, drawn"));
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

    assert!(before.bottom() > area.bottom() + px(100.), "{name} starts far past its area: {before:?} in {area:?}");
    assert!((after.bottom() - area.bottom()).abs() < px(2.), "{name} ends at its end: {after:?} in {area:?}");
}

/// What an area that scrolls a text has moved out of sight, such as the end of a compaction's long summary, can't be
/// pressed on, so a press below the area, across from where the summary's text would go on, is on what's drawn there.
#[gpui_kit::test]
fn a_press_below_a_long_summary_is_on_what_s_drawn_there(cx: &mut TestAppContext) {
    let (_fake, mut ui, summary) = with_a_long_summary(cx);

    // Indented less than the summary's list, so in from the left, under its text.
    let on_it = on_the_reply_saying(&ui, summary.window, LAST);
    click(&mut ui, point(on_it.x + px(100.), on_it.y), 2);
    let word = copy(&mut ui);

    assert!(!word.trim().is_empty() && LAST.contains(&word), "a word of the reply: {word:?}");
}

/// A drag that starts in a long summary and is held below its area scrolls the summary on to its end, as the wheel
/// would, and selects to there, and held above it, scrolls it back to its start. At either end, it rests.
#[gpui_kit::test]
fn a_drag_held_past_a_long_summary_scrolls_it_on_either_way(cx: &mut TestAppContext) {
    let (_fake, mut ui, summary) = with_a_long_summary(cx);
    let area = bounds(&mut ui, summary.area);
    // On the long item of its list, and then far enough past the area to get to each end soon.
    let start = point(area.left() + px(60.), area.top() + px(40.));
    let (below, above) = (point(start.x, area.bottom() + px(30.)), point(start.x, area.top() - px(30.)));
    let inside = point(start.x, area.bottom() - px(10.));

    let (button, held) = (MouseButton::Left, Some(MouseButton::Left));
    press(&mut ui, MouseDownEvent { button, position: start, click_count: 1, ..Default::default() });
    press(&mut ui, MouseMoveEvent { position: inside, pressed_button: held, ..Default::default() });
    ui.settle(A_FRAME);
    let in_sight = copy(&mut ui);
    press(&mut ui, MouseMoveEvent { position: below, pressed_button: held, ..Default::default() });
    ui.settle(A_FRAME);
    let down = copy(&mut ui);
    press(&mut ui, MouseMoveEvent { position: above, pressed_button: held, ..Default::default() });
    ui.settle(A_FRAME);
    let up = copy(&mut ui);
    press(&mut ui, MouseUpEvent { button, position: above, click_count: 1, ..Default::default() });

    // Held inside the area, the drag selects only what shows, which the end of the summary is past.
    assert!(!in_sight.is_empty() && !in_sight.contains(AFTER), "its end starts out of sight: {:?}", tail(&in_sight));
    assert!(down.ends_with(AFTER), "down to the end of the summary: {:?}", tail(&down));
    assert!(up.starts_with(FIRST), "and up to its start: {:?}", head(&up));
}

/// A drag up from the reply below a long summary that's been scrolled down, to above its area, selects from the
/// start of what shows of the summary, rather than from the start of what's moved out of sight above it, even with
/// the prompt that asked for it drawn further up.
#[gpui_kit::test]
fn a_drag_up_to_a_scrolled_summary_starts_at_what_shows_of_it(cx: &mut TestAppContext) {
    let (_fake, mut ui, summary) = with_a_long_summary(cx);
    // Taller, so the end of the prompt above the compaction is in sight too, then the summary down.
    let size = ui.cx.update_window(ui.handle, |_, window, _| window.bounds().size).unwrap();
    ui.cx.simulate_window_resize(ui.handle, gpui_kit::size(size.width, size.height + px(300.)));
    let window = settled_at_its_last_row(&mut ui);
    let area = bounds(&mut ui, summary.area.clone());
    assert!(area.top() > window.top() + px(150.), "with room above the summary: {area:?} in {window:?}");
    turn_at(&mut ui, area.center(), px(-300.));
    let area = bounds(&mut ui, summary.area);
    let from = on_the_reply_saying(&ui, window, LAST);
    let above = point(area.left() + px(60.), area.top() - px(4.));

    let (button, held) = (MouseButton::Left, Some(MouseButton::Left));
    press(&mut ui, MouseDownEvent { button, position: from, click_count: 1, ..Default::default() });
    press(&mut ui, MouseMoveEvent { position: above, pressed_button: held, ..Default::default() });
    press(&mut ui, MouseUpEvent { button, position: above, click_count: 1, ..Default::default() });
    let selected = copy(&mut ui);

    let words = KEPT.split_whitespace();
    assert!(words.clone().any(|word| selected.starts_with(word)), "from the list's words: {:?}", head(&selected));
}

/// A chat whose compaction has a summary far longer than its area, opened, above a reply.
struct Summary {
    /// Where the transcript is.
    window: Bounds<Pixels>,
    /// The summary's area.
    area: ElementId,
}

/// The app, open on a chat held by a fake hub with a long [`Summary`], as the hub records the compaction the fake
/// agent makes when it's asked to keep the summary's list and what follows it. The hub goes with the test, so it's
/// returned to be kept.
fn with_a_long_summary(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, Summary) {
    // About twice as tall as its area, so its end is out of sight below it, which the drag's test checks, with room for
    // a narrower font, and no taller, since a drag held past it goes through all of it, twice.
    let keep = format!("{}\n\n{AFTER}", KEPT.repeat(50));
    let summary = format!("1. {FIRST}: the prompts so far.\n2. What to keep: {keep}");
    let chat = turns(vec![
        ("say:Hello".to_owned(), vec![text("msg-hello", "Hello")]),
        ("/compact the timetables, the sidings and the junctions".to_owned(), vec![compacted(&summary)]),
        (format!("say:{LAST}"), vec![text("msg-last", LAST)]),
    ]);
    let (fake, mut ui, _, _) = open_on_chats_of(cx, vec![chat]);
    let compacted = ui.events().into_iter().find(|event| matches!(event.body, EventBody::AgentCompacted { .. }));
    let seq = compacted.unwrap().seq;
    ui.click(SharedString::from(format!("compaction/event/{seq}/0")));
    ui.keep_to_the_end();
    let window = settled_at_its_last_row(&mut ui);
    let area = ElementId::from(SharedString::from(format!("summary-scroll/event/{seq}/0")));
    (fake, ui, Summary { window, area: ElementId::from((area, "area")) })
}

/// Turns the wheel at `at` by `y`, down the page when it's less than nothing, as a gesture of its own.
fn turn_at(ui: &mut Ui<'_>, at: Point<Pixels>, y: Pixels) {
    ui.cx.executor().advance_clock(A_PAUSE);
    press(ui, MouseMoveEvent { position: at, ..Default::default() });
    press(ui, ScrollEvent { position: at, delta: ScrollDelta::Pixels(point(px(0.), y)), ..Default::default() });
    ui.settle(A_FRAME);
}
