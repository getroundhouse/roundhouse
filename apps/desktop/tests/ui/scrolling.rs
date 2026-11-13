//! Scrolling through a session longer than a transcript loads at once: by the wheel and by the scrollbar's thumb,
//! what the scrollbar says along the way, and how little of the session is measured for it.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, ScrollDelta, TestAppContext,
    point, px,
};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::SessionId;
use transcript::Place;

use crate::support::{Stack, Ui};

mod cost;
mod late_heights;
mod latest;
mod selecting;

/// A frame's length, for letting the list measure what it does a few rows to a frame.
const A_FRAME: Duration = Duration::from_millis(16);

#[gpui_kit::test]
fn long_history_loads_a_page_at_a_time_as_it_is_scrolled_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let newest = long_session(&stack);

    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let total = wait_for_the_tail(&mut ui, &newest);
    assert!(total > 700);
    // Only the tail so far. The start of the session isn't held.
    let loaded = ui.cx.read_entity(&transcript, |t, _| t.loaded_events());
    assert!(loaded < total, "{loaded} of {total}");
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_none()));

    ui.cx.update_entity(&transcript, |t, cx| {
        t.list().go_to_row(0);
        cx.notify();
    });
    ui.wait("the first page to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_some()));
    assert!(matches!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).unwrap().body), EventBody::UserPrompt { .. }));
}

#[gpui_kit::test]
fn the_scrollbar_s_thumb_goes_up_a_long_session_as_steadily_as_the_wheel_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Replies of a line, a paragraph and several long paragraphs, in no order, since rows alike hide what an
    // estimate gets wrong.
    let newest = session_saying(&stack, 700, |i| {
        let paragraphs = [1, 1, 3, 1, 6, 1, 2][i * 5 % 7];
        let sentences = [1, 2, 12, 1, 5][i * 3 % 5];
        vec!["the signal box keeps a ledger of every train. ".repeat(sentences); paragraphs].join("\n\n")
    });
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    wait_for_the_tail(&mut ui, &newest);
    ui.frame();
    assert!((thumb(&ui) - 1.).abs() < f32::EPSILON, "it starts at the end, not at {}", thumb(&ui));

    // All the way up, which takes in every page of the session's history.
    let read = |ui: &Ui<'_>| {
        ui.cx.read_entity(&transcript, |t, _| {
            let (thumb, tall) = t.list().thumb();
            Step { thumb, tall, in_the_log: t.loaded_rows(), in_the_list: t.list().loaded_rows() }
        })
    };
    let mut steps = vec![read(&ui)];
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().top());
    while top(&ui) != Place::default() {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(150.))));
        let row = top(&ui).row;
        wait_for_the_list_to_settle(&mut ui, row);
        steps.push(read(&ui));
        assert!(steps.len() < 5000, "the top is never reached");
    }

    // The wheel went as far as the session is tall, so the list passed through every row and jumped over none. A
    // long prompt or reply was once measured before its text had been read, and skipped.
    let (travelled, tall) = (px(150.) * (steps.len() - 1), steps[steps.len() - 1].tall);
    assert!(travelled > tall * 0.95 && travelled < tall * 1.05, "{travelled:?} of wheel through {tall:?}");
    assert!(thumb(&ui).abs() < f32::EPSILON, "it ends at the top, not at {}", thumb(&ui));
    let steady = went_up_steadily(&steps);
    assert!(steady > steps.len() / 2, "only {steady} of {} steps had no page arriving to excuse them", steps.len());

    // With every row loaded and measured, back down is as steady as can be.
    let mut down = vec![thumb(&ui)];
    while thumb(&ui) < 1. {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(-150.))));
        down.push(thumb(&ui));
        assert!(down.len() < 5000, "the end is never reached");
    }
    let mut moved: Vec<f32> = down.windows(2).map(|pair| pair[1] - pair[0]).collect();
    moved.pop();
    let (least, most) = moved.iter().fold((f32::MAX, 0f32), |(least, most), step| (least.min(*step), most.max(*step)));
    assert!(most - least < most * 0.01, "steps from {least} to {most}");
}

/// What the scrollbar says after a turn of the wheel, and how much of the session is loaded.
struct Step {
    /// How far down its track the thumb is, from 0 to 1.
    thumb: f32,
    /// How tall the scrollbar takes the session to be.
    tall: Pixels,
    /// How many rows the session's log holds, with the row that shows what's streaming.
    in_the_log: usize,
    /// How many of them the list has been told of. A page goes into the log on another thread, and the list is told
    /// once the pages asked for with it are all in. Rows drawn from the log meanwhile are measured as they're drawn.
    in_the_list: usize,
}

impl Step {
    /// Whether a page is on its way in: the log holds rows the list hasn't been told of.
    fn is_loading(&self) -> bool {
        self.in_the_log != self.in_the_list
    }
}

/// The thumb only ever goes up, and as far each time, and the session's height stays as it is, but for while an
/// older page arrives and takes the place of the guess at it. Then the thumb moves by however unlike the guess the
/// page is.
///
/// Returns how many steps were held to that, with no page arriving.
fn went_up_steadily(steps: &[Step]) -> usize {
    let (mut usual, mut steady): (Option<f32>, usize) = (None, 0);
    // The last step stops at the top, however far that is.
    for (step, pair) in steps[..steps.len() - 1].windows(2).enumerate() {
        let (before, after) = (&pair[0], &pair[1]);
        let paged = before.in_the_list != after.in_the_list || before.is_loading() || after.is_loading();
        assert!(
            before.tall == after.tall || paged,
            "the session's height went from {:?} to {:?} with no page arriving",
            before.tall,
            after.tall
        );
        if paged || before.tall != after.tall {
            usual = None;
            continue;
        }
        let distance = before.thumb - after.thumb;
        let usual = *usual.get_or_insert(distance);
        assert!(
            (distance - usual).abs() < usual * 0.01,
            "step {step} went {distance} where those before it went {usual}"
        );
        steady += 1;
    }
    steady
}

#[gpui_kit::test]
fn dragging_the_scrollbar_s_thumb_stops_part_way_through_a_long_reply(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Three replies, each several screens long.
    let newest = session_saying(&stack, 3, |_| vec!["the signal box keeps a ledger. ".repeat(40); 30].join("\n\n"));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    wait_for_the_tail(&mut ui, &newest);
    // The bar shows once the transcript has moved.
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(20.))));
    let bar = ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap();
    let thumb_at = point(bar.right() - px(8.), bar.bottom() - px(30.));
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().top());
    let before = top(&ui);

    ui.cx
        .update_window(ui.handle, |_, window, cx| window.drag(thumb_at, point(thumb_at.x, thumb_at.y - px(40.)), cx))
        .unwrap();
    ui.frame();

    let after = top(&ui);
    assert_eq!(after.row, before.row, "a short drag stays in the same reply");
    assert!(after.offset > px(0.) && after.offset < before.offset, "{before:?} to {after:?}");
}

#[gpui_kit::test]
fn the_scrollbar_s_thumb_keeps_its_length_through_a_few_very_long_rows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // A prompt screens long, since it holds the script, and three replies as long. Whichever is drawn first says
    // little about the height of the rest unless their lengths are gone by.
    let newest = session_saying(&stack, 3, |_| vec!["the signal box keeps a ledger. ".repeat(40); 30].join("\n\n"));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    wait_for_the_tail(&mut ui, &newest);
    ui.frame();
    let height = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().thumb().1);
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().top());

    let mut heights = vec![height(&ui)];
    while top(&ui) != Place::default() {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
        heights.push(height(&ui));
        assert!(heights.len() < 2000, "the top is never reached");
    }

    let lurch = heights.windows(2).map(|pair| ((pair[1] - pair[0]) / pair[0]).abs()).fold(0., f32::max);
    assert!(lurch < 0.05, "the session's height changed by {lurch} in a step");
    let (least, most) = heights.iter().fold((heights[0], heights[0]), |(least, most), h| (least.min(*h), most.max(*h)));
    assert!(most < least * 1.25, "the session's height went from {least:?} to {most:?}");
}

#[gpui_kit::test]
fn dragging_the_scrollbar_s_thumb_goes_through_the_session_only_the_way_the_thumb_goes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Short prompts, and replies from a line to a few screens long, more than a transcript loads at once.
    let newest = session_saying(&stack, 400, |i| format!("prose:{}:{i}", [1, 2, 8, 1, 3, 20][i % 6]));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let total = wait_for_the_tail(&mut ui, &newest);
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_none()), "the start isn't loaded yet");
    // The bar shows once the transcript has moved, with the thumb at the bottom of its track.
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(20.))));
    let bar = ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap();
    let mut at = point(bar.right() - px(8.), bar.bottom() - px(30.));
    let top = |ui: &Ui<'_>| {
        let top = ui.cx.read_entity(&transcript, |t, _| t.list().top());
        (top.row, top.offset)
    };
    press(&mut ui, MouseDownEvent { button: MouseButton::Left, position: at, click_count: 1, ..Default::default() });

    // Up the track a few pixels at a time, as a hand does, stopping half way for the rows there to load.
    let mut tops = vec![top(&ui)];
    let mut half_way = None;
    while at.y > bar.top() + px(30.) {
        at.y -= px(4.);
        let held = Some(MouseButton::Left);
        press(&mut ui, MouseMoveEvent { position: at, pressed_button: held, ..Default::default() });
        if half_way.is_none() && at.y < bar.center().y {
            half_way = Some((thumb(&ui), top(&ui).0));
            let row = top(&ui).0;
            ui.wait("the rows there to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_some()));
        }
        tops.push(top(&ui));
    }
    press(&mut ui, MouseUpEvent { button: MouseButton::Left, position: at, click_count: 1, ..Default::default() });

    let (fraction, row) = half_way.unwrap();
    assert!((0.4..0.6).contains(&fraction), "half way up the track, the thumb is at {fraction}");
    assert!((total * 2 / 5..total * 3 / 5).contains(&row), "and row {row} of {total} is at the top");
    // Older pages arrive as the thumb reaches them, and take the place of the guess at them, which can move what's
    // under the thumb a little either way. Otherwise the transcript only goes up.
    let back: Vec<_> = tops.windows(2).filter(|pair| pair[1] > pair[0]).collect();
    assert!(back.len() <= total / 250 + 1, "the transcript went back down {back:?}");
    assert!(
        tops[tops.len() - 1].0 < total / 10,
        "and the thumb near the top shows the start: {:?}",
        tops[tops.len() - 1]
    );
}

/// A long session, open at its end with the scrollbar showing, and everything loaded so far measured.
fn open_on_a_long_session<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let newest = session_saying(stack, 700, |i| "the signal box keeps a ledger. ".repeat([1, 4, 30][i % 3]));
    let mut ui = Ui::open(cx, stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let last = ui.row_of(&newest);
    wait_for_the_list_to_settle(&mut ui, last);
    ui
}

/// Waits for the list to settle with the last event the chat of `session` shows, the newest of the agent's own as the
/// hub recorded it, loaded in its row.
fn settle_at_the_end(ui: &mut Ui<'_>, stack: &Stack, session: &SessionId) {
    let last = ui.row_of(&stack.newest_event(session));
    wait_for_the_list_to_settle(ui, last);
}

/// Waits for `newest`, the newest of the agent's own events as the hub recorded it, to load, which it does live as the
/// chat opens at its end, and says how many rows the chat has, as its log says.
fn wait_for_the_tail(ui: &mut Ui<'_>, newest: &SessionEvent) -> usize {
    let (transcript, last) = (ui.transcript().unwrap(), ui.row_of(newest));
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(last).is_some()));
    last + 1
}

/// Waits for `row` to be loaded and for the list to settle with it. A page changes the session's height in three
/// goes: as its rows in the window are measured, over the frames after as the rest of them are, a few to a frame,
/// and as each long text in it is parsed on another thread and its row measured again. So the list has settled once
/// every long text is parsed and no frame is asked for. Nothing is drawn after that, since that could bring another
/// page.
fn wait_for_the_list_to_settle(ui: &mut Ui<'_>, row: usize) {
    let transcript = ui.transcript().unwrap();
    ui.wait("the rows there to load and the list to settle", |ui| {
        ui.settle(A_FRAME);
        ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_some() && t.long_texts_parsed())
    });
}

/// The scrollbar's thumb is put where the pointer is, and the list goes to the row that's that far down the session.
/// Rows load under the thumb and turn out other heights than they were taken to be, and how far down a row is goes
/// with them. Going by that while the thumb is held moved the page under a pointer that was holding still.
#[gpui_kit::test]
fn a_place_on_the_scrollbars_track_is_the_same_row_however_long_the_thumb_rests_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_on_a_long_session(cx, &stack);
    let transcript = ui.transcript().unwrap();
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(20.))));
    let bar = ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap();
    let from = point(bar.right() - px(8.), bar.bottom() - px(30.));
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().top().row);
    let tall = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().thumb().1);
    let held = Some(MouseButton::Left);
    // How tall the session is taken to be with only its end loaded, before the thumb asks for any more of it.
    let before = tall(&ui);
    press(&mut ui, MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() });

    // Up to the middle of the track, into rows that haven't been fetched. The scrollbar draws a drag no more often
    // than the display does, so the list is drawn there a frame on.
    let rest = point(from.x, bar.center().y);
    press(&mut ui, MouseMoveEvent { position: rest, pressed_button: held, ..Default::default() });
    ui.play(A_FRAME);
    let row = top(&ui);
    ui.wait("the rows there to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_some()));

    // The pointer says where it is again and again, as a hand resting on it does, while the rows that loaded are
    // measured a few to a frame.
    let mut tops = vec![top(&ui)];
    for _ in 0..40 {
        press(&mut ui, MouseMoveEvent { position: rest, pressed_button: held, ..Default::default() });
        ui.play(A_FRAME);
        tops.push(top(&ui));
    }

    assert_ne!(tall(&ui), before, "the rows that loaded aren't as tall as they were taken to be");
    assert!(tops.iter().all(|top| top.abs_diff(row) <= 1), "the page moved under a thumb held still: {tops:?}");
}

/// Sends the window a pointer event, and lets it draw.
fn press(ui: &mut Ui<'_>, event: impl gpui_kit::InputEvent) {
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event.to_platform_input(), cx)).unwrap();
    ui.frame();
}

/// How far down the session the transcript's scrollbar says it is.
fn thumb(ui: &Ui<'_>) -> f32 {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().thumb().0)
}

/// Starts a session and has the agent say 700 things in it, which is more than a transcript loads at once. Returns
/// the newest of the agent's own events in it, as the hub recorded it.
fn long_session(stack: &Stack) -> SessionEvent {
    session_saying(stack, 700, |i| format!("message {i}"))
}

/// Starts a session and has the agent say `count` things in it, over as many prompts as that takes. Returns the newest
/// of the agent's own events in it, as the hub recorded it.
fn session_saying(stack: &Stack, count: usize, say: impl Fn(usize) -> String) -> SessionEvent {
    let session = stack.chat_in_a_new_workspace();
    // A few to a prompt, as a person would ask, so no prompt is itself most of the session.
    let script: Vec<String> = (0..count).map(|i| format!("say:{}", say(i))).collect();
    for prompt in script.chunks(25) {
        stack.run(&session, &prompt.join("; "));
    }
    stack.newest_event(&session)
}
