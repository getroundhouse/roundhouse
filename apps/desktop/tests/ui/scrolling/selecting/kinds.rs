//! Selecting each kind of text the chat records, besides the agent's replies: a thought while it's open, a plan, and
//! what shows of a compaction's summary while it's open, whose area scrolls on under a drag from it. A prompt is
//! selected with the rest, as `super` checks.

use std::time::Duration;

use gpui_kit::{
    AppContext as _, Bounds, ElementId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollDelta, ScrollEvent, SharedString, TestAppContext, point, px,
};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::SessionStatus;

use super::super::{A_FRAME, press};
use super::{LAST, bounds, click, copy, everything, head, on_the_reply_saying, open, settled, tail};
use crate::support::{Stack, Ui};

/// What a long summary's list is made of, over and over, in words the reply doesn't have.
const KEPT: &str = "timetables sidings junctions ";
/// What follows the list, in words the reply doesn't have either, which starts out of sight.
const AFTER: &str = "Junctions, sidings, timetables.";
/// How the summary's list starts, out of sight once it's scrolled down.
const FIRST: &str = "What was asked";
/// Longer than a wheel's turns take to count as one gesture, so the next turn goes to what it's over.
const A_PAUSE: Duration = Duration::from_secs(1);

/// A thought is copied with the rest while it's open, and isn't once it's closed, since it doesn't show then.
#[gpui_kit::test]
fn a_thought_is_selected_with_the_rest_only_while_it_s_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("ponder:1:7; say:{LAST}"));
    let mut ui = open(cx, &stack);
    let transcript = ui.transcript().unwrap();
    settled(&mut ui, &stack, &session);
    let thought = thought(&ui);

    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    let window = settled(&mut ui, &stack, &session);
    let on_it = on_the_reply_saying(&ui, window, LAST);
    let opened = everything(&mut ui, on_it);
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(false, cx));
    let window = settled(&mut ui, &stack, &session);
    let on_it = on_the_reply_saying(&ui, window, LAST);
    let closed = everything(&mut ui, on_it);

    let start: String = thought.chars().take(40).collect();
    assert!(opened.contains(&start), "opened, the thought is copied: {opened:?}");
    assert!(!closed.contains(&start), "closed, it isn't: {closed:?}");
    assert!(closed.ends_with(LAST), "though the rest is");
}

/// A plan the person was asked to approve is selected with the rest, once it's approved too.
#[gpui_kit::test]
fn a_plan_is_selected_with_the_rest(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let plan = "Timetables first, then the sidings, then each junction.";
    stack.client().session_prompt(&session, &format!("plan:{plan}"), None).unwrap();
    let mut ui = open(cx, &stack);
    let approval = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("approve-plan".into(), approval));
    ui.wait("the plan's turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    stack.run(&session, &format!("say:{LAST}"));
    let window = settled(&mut ui, &stack, &session);
    let on_it = on_the_reply_saying(&ui, window, LAST);

    let all = everything(&mut ui, on_it);

    // The prompt has the plan's words too, after what asks for it.
    assert!(all.contains(&format!("\n\n{plan}\n\n")), "the plan, between what's before and after it: {all:?}");
    assert!(all.ends_with(LAST), "to the last reply");
}

/// A summary is copied with the rest while it's open, and isn't once it's closed.
#[gpui_kit::test]
fn a_summary_is_selected_with_the_rest_only_while_it_s_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, summary) = with_a_long_summary(cx, &stack);
    let on_it = on_the_reply_saying(&ui, summary.window, LAST);
    let opened = everything(&mut ui, on_it);

    // Up to the compaction's line, which the opened summary takes above the window.
    turn_at(&mut ui, on_it, px(80.));
    ui.click(summary.line.clone());
    let window = settled(&mut ui, &stack, &summary.session);
    let on_it = on_the_reply_saying(&ui, window, LAST);
    let closed = everything(&mut ui, on_it);

    // The prompt that asked for the compaction says it too.
    assert_eq!(opened.matches(AFTER).count(), 2, "opened, the summary is copied");
    assert_eq!(closed.matches(AFTER).count(), 1, "closed, it isn't");
}

/// What an area that scrolls a text has moved out of sight, such as the end of a compaction's long summary, can't be
/// pressed on, so a press below the area, across from where the summary's text would go on, is on what's drawn there.
#[gpui_kit::test]
fn a_press_below_a_long_summary_is_on_what_s_drawn_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, summary) = with_a_long_summary(cx, &stack);

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
    let stack = Stack::new();
    let (mut ui, summary) = with_a_long_summary(cx, &stack);
    let area = bounds(&mut ui, summary.area);
    // On the long item of its list, and then far enough past the area to get to each end soon.
    let start = point(area.left() + px(60.), area.top() + px(40.));
    let (below, above) = (point(start.x, area.bottom() + px(30.)), point(start.x, area.top() - px(30.)));

    let (button, held) = (MouseButton::Left, Some(MouseButton::Left));
    press(&mut ui, MouseDownEvent { button, position: start, click_count: 1, ..Default::default() });
    press(&mut ui, MouseMoveEvent { position: below, pressed_button: held, ..Default::default() });
    ui.settle(A_FRAME);
    let down = copy(&mut ui);
    press(&mut ui, MouseMoveEvent { position: above, pressed_button: held, ..Default::default() });
    ui.settle(A_FRAME);
    let up = copy(&mut ui);
    press(&mut ui, MouseUpEvent { button, position: above, click_count: 1, ..Default::default() });

    assert!(down.ends_with(AFTER), "down to the end of the summary: {:?}", tail(&down));
    assert!(up.starts_with(FIRST), "and up to its start: {:?}", head(&up));
}

/// A drag up from the reply below a long summary that's been scrolled down, to above its area, selects from the
/// start of what shows of the summary, rather than from the start of what's moved out of sight above it, even with
/// the prompt that asked for it drawn further up.
#[gpui_kit::test]
fn a_drag_up_to_a_scrolled_summary_starts_at_what_shows_of_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, summary) = with_a_long_summary(cx, &stack);
    // Taller, so the end of the prompt above the compaction is in sight too, then the summary down.
    let size = ui.cx.update_window(ui.handle, |_, window, _| window.bounds().size).unwrap();
    ui.cx.simulate_window_resize(ui.handle, gpui_kit::size(size.width, size.height + px(300.)));
    let window = settled(&mut ui, &stack, &summary.session);
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
    session: protocol::model::SessionId,
    /// Where the transcript is.
    window: Bounds<Pixels>,
    /// The summary's area.
    area: ElementId,
    /// The compaction's line, which opens and closes it.
    line: SharedString,
}

/// The app, open on a chat with a long [`Summary`].
fn with_a_long_summary<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, Summary) {
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:Hello");
    stack.run(&session, &format!("/compact {}\n\n{AFTER}", KEPT.repeat(200)));
    stack.run(&session, &format!("say:{LAST}"));
    let mut ui = open(cx, stack);
    settled(&mut ui, stack, &session);
    let compacted = ui.events().into_iter().find(|event| matches!(event.body, EventBody::AgentCompacted { .. }));
    let seq = compacted.unwrap().seq;
    let line = SharedString::from(format!("compaction/event/{seq}/0"));
    ui.click(line.clone());
    let window = settled(&mut ui, stack, &session);
    let area = ElementId::from(SharedString::from(format!("summary-scroll/event/{seq}/0")));
    (ui, Summary { session, window, area: ElementId::from((area, "area")), line })
}

/// Turns the wheel at `at` by `y`, down the page when it's less than nothing, as a gesture of its own.
fn turn_at(ui: &mut Ui<'_>, at: Point<Pixels>, y: Pixels) {
    ui.cx.executor().advance_clock(A_PAUSE);
    press(ui, MouseMoveEvent { position: at, ..Default::default() });
    press(ui, ScrollEvent { position: at, delta: ScrollDelta::Pixels(point(px(0.), y)), ..Default::default() });
    ui.settle(A_FRAME);
}

/// A prompt sent while the agent thinks is recorded before the thought, but the agent takes it only after, so it's
/// drawn after the thought, where it was taken, and copied there too.
#[gpui_kit::test]
fn a_prompt_taken_after_a_thought_is_copied_after_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let transcript = ui.transcript().unwrap();
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    ui.input("think:30:idea");
    ui.press("enter");
    ui.wait("the thought to stream", |ui| ui.cx.read_entity(&transcript, |t, _| t.streaming_text().contains("idea3")));
    let steered = format!("say:{LAST}");
    ui.input(&steered);
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains(LAST));

    let session = ui.active_chat();
    let window = settled(&mut ui, &stack, &session);
    let on_it = on_the_reply_saying(&ui, window, LAST);
    let copied = everything(&mut ui, on_it);

    // Drawn, the thought doesn't end with the space it was recorded with.
    let thought = thought(&ui);
    let thought = thought.trim_end();
    assert_eq!(copied, format!("think:30:idea\n\n{thought}\n\n{steered}\n\n{LAST}"), "in the order they're drawn");
}

/// What the agent thought in the chat `ui` shows.
fn thought(ui: &Ui<'_>) -> String {
    let thought = ui.events().into_iter().find_map(|event| match event.body {
        EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
            ContentBlock::Thinking { text, .. } => Some(text.clone()),
            _ => None,
        }),
        _ => None,
    });
    thought.unwrap()
}
