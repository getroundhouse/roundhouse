//! What a session shows as it runs.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ListOffset, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta,
    TestAppContext, point, px,
};
use protocol::event::EventBody;
use protocol::model::{SessionStatus, seq_index};

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn text_streams_in_as_it_is_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("slow:20:word");
    ui.press("enter");
    ui.wait("streaming text", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("word5")))
    });
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));
    ui.wait_idle_with("the finished message", |text| text.contains("word19"));
    assert!(ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().is_empty())));
}

#[gpui_kit::test]
fn the_stop_button_stops_the_turn(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:go; hang:60000");
    ui.press("enter");
    ui.wait("the agent to start", |ui| ui.assistant_text().contains("go"));
    ui.wait("the Stop button", |ui| {
        ui.session_status() == Some(SessionStatus::WaitingApproval)
            || ui.session_status() == Some(SessionStatus::Running)
    });
    ui.click("stop");
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    // The session's status and its log arrive separately, in either order.
    ui.wait("the cancelled turn in the log", |ui| {
        ui.events()
            .iter()
            .any(|e| matches!(e.body, EventBody::TurnEnded { outcome: protocol::event::TurnOutcome::Cancelled, .. }))
    });
}

#[gpui_kit::test]
fn long_history_loads_a_page_at_a_time_as_it_is_scrolled_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let total = long_session(&stack);
    assert!(total > 700);

    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(total - 1).is_some()));
    // Only the tail so far. The start of the session isn't held.
    let loaded = ui.cx.read_entity(&transcript, |t, _| t.loaded_events());
    assert!(loaded < total, "{loaded} of {total}");
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_none()));

    ui.cx.update_entity(&transcript, |t, cx| {
        t.scroller().scroll_to_item(0);
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
    let total = session_saying(&stack, 700, |i| {
        let paragraphs = [1, 1, 3, 1, 6, 1, 2][i * 5 % 7];
        let sentences = [1, 2, 12, 1, 5][i * 3 % 5];
        vec!["the signal box keeps a ledger of every train. ".repeat(sentences); paragraphs].join("\n\n")
    });
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(total - 1).is_some()));
    ui.frame();
    assert!((thumb(&ui) - 1.).abs() < f32::EPSILON, "it starts at the end, not at {}", thumb(&ui));

    // All the way up, which takes in every page of the session's history.
    let mut steps = vec![thumb(&ui)];
    let mut lengths = vec![ui.cx.read_entity(&transcript, |t, _| t.scroller().thumb().1)];
    let mut loaded = vec![ui.cx.read_entity(&transcript, |t, _| t.loaded_events())];
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.scroller().logical_scroll_top());
    while top(&ui) != ListOffset::default() {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(150.))));
        let row = top(&ui).item_ix;
        ui.wait("the rows there to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(row).is_some()));
        steps.push(thumb(&ui));
        lengths.push(ui.cx.read_entity(&transcript, |t, _| t.scroller().thumb().1));
        loaded.push(ui.cx.read_entity(&transcript, |t, _| t.loaded_events()));
        assert!(steps.len() < 5000, "the top is never reached");
    }

    // The wheel went as far as the session is tall, so the list passed through every row and jumped over none. A
    // long prompt or reply was once measured before its text had been read, and skipped.
    let (travelled, tall) = (lengths.iter().skip(1).fold(px(0.), |far, _| far + px(150.)), lengths[lengths.len() - 1]);
    assert!(travelled > tall * 0.95 && travelled < tall * 1.05, "{travelled:?} of wheel through {tall:?}");
    assert!(thumb(&ui).abs() < f32::EPSILON, "it ends at the top, not at {}", thumb(&ui));
    // The thumb only ever goes up, and as far each time, and the session's height stays as it is, but for when an
    // older page arrives and takes the place of the guess at it. Then the thumb moves by however unlike the guess
    // the page is.
    let mut moved: Vec<f32> = steps.windows(2).map(|pair| pair[0] - pair[1]).collect();
    moved.pop();
    let mut usual: Option<f32> = None;
    for (step, distance) in moved.iter().enumerate() {
        let (paged, before, after) = (loaded[step] != loaded[step + 1], lengths[step], lengths[step + 1]);
        assert!(
            before == after || paged,
            "the session's height went from {before:?} to {after:?} with no page arriving"
        );
        if paged || before != after {
            usual = None;
            continue;
        }
        let usual = *usual.get_or_insert(*distance);
        assert!(
            (distance - usual).abs() < usual * 0.01,
            "step {step} went {distance} where those before it went {usual}"
        );
    }

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

#[gpui_kit::test]
fn dragging_the_scrollbar_s_thumb_stops_part_way_through_a_long_reply(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Three replies, each several screens long.
    let total = session_saying(&stack, 3, |_| vec!["the signal box keeps a ledger. ".repeat(40); 30].join("\n\n"));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(total - 1).is_some()));
    // The bar shows once the transcript has moved.
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(20.))));
    let bar = ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap();
    let thumb_at = point(bar.right() - px(8.), bar.bottom() - px(30.));
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.scroller().logical_scroll_top());
    let before = top(&ui);

    ui.cx
        .update_window(ui.handle, |_, window, cx| window.drag(thumb_at, point(thumb_at.x, thumb_at.y - px(40.)), cx))
        .unwrap();
    ui.frame();

    let after = top(&ui);
    assert_eq!(after.item_ix, before.item_ix, "a short drag stays in the same reply");
    assert!(after.offset_in_item > px(0.) && after.offset_in_item < before.offset_in_item, "{before:?} to {after:?}");
}

#[gpui_kit::test]
fn the_scrollbar_s_thumb_keeps_its_length_through_a_few_very_long_rows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // A prompt screens long, since it holds the script, and three replies as long. Whichever is drawn first says
    // little about the height of the rest unless their lengths are gone by.
    let total = session_saying(&stack, 3, |_| vec!["the signal box keeps a ledger. ".repeat(40); 30].join("\n\n"));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(total - 1).is_some()));
    ui.frame();
    let height = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.scroller().thumb().1);
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.scroller().logical_scroll_top());

    let mut heights = vec![height(&ui)];
    while top(&ui) != ListOffset::default() {
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
    let total = session_saying(&stack, 400, |i| format!("prose:{}:{i}", [1, 2, 8, 1, 3, 20][i % 6]));
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.wait("the tail to load", |ui| ui.cx.read_entity(&transcript, |t, _| t.event_at(total - 1).is_some()));
    assert!(ui.cx.read_entity(&transcript, |t, _| t.event_at(0).is_none()), "the start isn't loaded yet");
    // The bar shows once the transcript has moved, with the thumb at the bottom of its track.
    ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(20.))));
    let bar = ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap();
    let mut at = point(bar.right() - px(8.), bar.bottom() - px(30.));
    let top = |ui: &Ui<'_>| {
        let top = ui.cx.read_entity(&transcript, |t, _| t.scroller().logical_scroll_top());
        (top.item_ix, top.offset_in_item)
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

/// Sends the window a pointer event, and lets it draw.
fn press(ui: &mut Ui<'_>, event: impl gpui_kit::InputEvent) {
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event.to_platform_input(), cx)).unwrap();
    ui.frame();
}

/// How far down the session the transcript's scrollbar says it is.
fn thumb(ui: &Ui<'_>) -> f32 {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.scroller().thumb().0)
}

/// Starts a session and has the agent say 700 things in it, which is more than a transcript loads at once. Returns
/// how many rows that came to.
fn long_session(stack: &Stack) -> usize {
    session_saying(stack, 700, |i| format!("message {i}"))
}

/// Starts a session and has the agent say `count` things in it, over as many prompts as that takes. Returns how many
/// rows that came to.
fn session_saying(stack: &Stack, count: usize, say: impl Fn(usize) -> String) -> usize {
    let session = stack.chat_in_full_access();
    // A few to a prompt, as a person would ask, so no prompt is itself most of the session.
    let script: Vec<String> = (0..count).map(|i| format!("say:{}", say(i))).collect();
    for prompt in script.chunks(25) {
        stack.run(&session, &prompt.join("; "));
    }
    seq_index(stack.client().session_get(&session).unwrap().last_seq)
}

#[gpui_kit::test]
fn empty_prompts_are_not_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.press("enter");
    ui.input("   ");
    ui.press("enter");
    ui.frame();
    assert!(ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.row_count() == 0)));
    // They never reach the hub, so there's nothing to complain about either.
    assert_eq!(ui.view().error, None);
}

#[gpui_kit::test]
fn a_sent_prompt_shows_once_and_becomes_the_recorded_event(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("hello"));
    let transcript = ui.transcript().unwrap();
    let rows = ui.cx.read_entity(&transcript, |t, _| t.row_count());
    let recorded = ui.events().len();
    assert_eq!(rows, recorded, "nothing is left waiting to be recorded");
    let prompts = ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 1);
}

#[gpui_kit::test]
fn a_turn_says_how_long_its_taken_until_it_ends_and_then_copies_its_reply(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("slow:20:word");
    ui.press("enter");

    ui.wait("the working line", |ui| ui.has("working-line"));
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));
    ui.wait_idle_with("the finished message", |text| text.contains("word19"));
    assert!(!ui.has("working-line"), "it goes with the turn");
    // The session's status and its log arrive separately, in either order.
    let ended = |ui: &Ui| ui.events().iter().find(|e| matches!(e.body, EventBody::TurnEnded { .. })).map(|e| e.seq);
    ui.wait("the end of the turn in the log", |ui| ended(ui).is_some());
    ui.click(gpui_kit::SharedString::from(format!("copy-reply/event/{}/0", ended(&ui).unwrap())));

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("done"), "what the fake agent says its turn came to");
}
