//! What a session shows as it runs.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::{AppContext as _, Modifiers, MouseMoveEvent, TestAppContext, point, px};
use protocol::event::EventBody;
use protocol::model::{SessionId, SessionStatus};

use crate::support::{Gate, Stack, Ui, wait_until};
use crate::tools::in_a_new_workspace;

#[gpui_kit::test]
fn text_streams_in_as_it_is_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    // A reply that goes on until the test has seen it stream.
    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("slow-until:{done}:word"));
    ui.press("enter");
    ui.wait("streaming text", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("word5")))
    });
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));
    done.open();
    ui.wait_idle_with("the finished message", |text| text.contains("word5"));
    assert!(ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().is_empty())));
}

/// Once a reply's recorded, it's parsed whole. The rest of it can arrive with the record, after the stream is gone
/// from the tail, and it shows all of that too, rather than only what streamed.
#[gpui_kit::test]
fn a_reply_shows_all_of_itself_once_it_is_recorded_however_its_end_arrived(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let done = Gate::new(&stack, "done");
    stack.client().session_prompt(&session, &format!("slow-until:{done}:word"), None).unwrap();
    // Drawn, rather than only in the session's log, which a thread of its own fills ahead of what the app has drawn.
    ui.wait("the start of the reply to be drawn", |ui| {
        ui.cx.read_entity(&transcript, |t, _| t.kept_texts()).iter().any(|kept| kept.contains("word0"))
    });
    // The chunks the agent has gone on from, and the one it may be saying, come before this one, so the app hasn't
    // drawn it.
    let said = done.said();
    let undrawn = format!("word{}", said + 1);

    // The app doesn't run while these wait, so it takes in the rest of the reply and its record in one go. The hub
    // sends the end of the stream as soon as it's written, and the record once it's stored, so the turn is over
    // only once the app's log, which a thread of its own fills, holds the record too.
    wait_until("the agent to have said what the app hasn't drawn", || done.said() > said + 1);
    done.open();
    wait_for_the_whole_turn(&stack, &ui, &session);
    ui.wait_idle_with("the recorded reply", |text| text.contains(&undrawn));
    ui.settle(Duration::from_millis(16));

    let reply = ui.assistant_text();
    let shown = ui.cx.read_entity(&transcript, |t, _| t.kept_texts());
    assert!(shown.contains(&reply), "{reply:?} shows as {shown:?}");
}

/// A long reply, which the transcript parses on another thread once it's recorded, goes on showing what it was parsed
/// as while it streamed while that's done, rather than going blank between. So does one drawn once the log's record of
/// it has taken it from the end of the chat, and before the transcript has caught up with the log.
#[gpui_kit::test]
fn a_long_reply_never_goes_blank_as_it_is_recorded(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    // Over 5 KB in pieces of about 800 bytes, which is more than is parsed where it's drawn, going on until the test
    // has seen that much stream.
    let phrase = "the signal box keeps a ledger of every train that passes, and the lever frame holds each route clear \
        until the train has gone by, so the ledger says which train took which route, and this is part";
    let done = Gate::new(&stack, "done");
    let script = format!("slow-until:{done}:{}{phrase}", format!("{phrase} ").repeat(3));
    stack.client().session_prompt(&session, &script, None).unwrap();
    ui.wait("the reply to stream", |ui| ui.cx.read_entity(&transcript, |t, _| t.streaming_text().len() > 5 * 1024));
    done.open();
    let last = wait_for_the_whole_turn(&stack, &ui, &session);
    // Before the transcript has caught up with the log, which has taken the reply from the end of the chat.
    ui.redraw();
    let shown = ui.cx.read_entity(&transcript, |t, _| t.row_count());
    assert!(shown <= last, "drawn from the log from before the turn ended, with the reply at the end of the chat");
    ui.wait_idle_with("the recorded reply", |text| text.contains("part3"));
    ui.settle(Duration::from_millis(16));

    let heights = ui.cx.read_entity(&transcript, |t, _| {
        let reply = |row: &usize| t.event_at(*row).is_some_and(|event| says(&event.body, "part3"));
        let row = (0..t.row_count()).find(reply).expect("the recorded reply");
        t.list().heights_measured(row)
    });
    let last = *heights.last().unwrap();
    let tall = heights.iter().position(|height| *height > last / 2.).unwrap();
    assert!(tall + 1 < heights.len(), "it was drawn as it streamed, before it was parsed: {heights:?}");
    assert!(heights[tall..].iter().all(|height| *height > last / 2.), "and never went blank after: {heights:?}");
}

/// A frame is drawn from the log as the transcript last caught up with it, and the log can take a reply's record while
/// the frame is drawn, which takes the reply from the end of the chat. The end of the chat goes on drawing it as it
/// streamed until the transcript catches up, rather than it being drawn in neither.
#[gpui_kit::test]
fn a_reply_recorded_while_a_frame_is_drawn_stays_at_the_end_for_that_frame(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    let done = Gate::new(&stack, "done");
    stack.client().session_prompt(&session, &format!("slow-until:{done}:word"), None).unwrap();
    ui.wait("the reply to be drawn", |ui| ui.cx.read_entity(&transcript, |t, _| t.all_as_copied().contains("word2")));
    done.open();
    wait_for_the_whole_turn(&stack, &ui, &session);

    let handle = ui.handle;
    ui.cx
        .update_window(handle, |_, window, cx| {
            transcript.update(cx, |t, cx| {
                t.start_a_frame();
                let end = t.list().row_count() - 1;
                t.draw_row_in_this_frame(end, window, cx);
            });
        })
        .unwrap();
    let copied = ui.cx.read_entity(&transcript, |t, _| t.all_as_copied());
    assert!(copied.contains("word2"), "the reply is still shown: {copied:?}");
}

/// Waits, without letting the app run, for the app's log to hold all of the turn the agent has just ended, so nothing
/// more comes to it until the next prompt, and says which row has the last of it. The turn's end has the chat idle, and
/// the agent says it's idle after that, the last it sends of a turn, which comes to the log as a row of its own.
fn wait_for_the_whole_turn(stack: &Stack, ui: &Ui, session: &SessionId) -> usize {
    stack.wait_for_whole_turn(session);
    let last = ui.row_of(&stack.newest_event(session));
    let transcript = ui.transcript().unwrap();
    wait_until("the turn to reach the app", || ui.cx.read_entity(&transcript, |t, _| t.event_at(last).is_some()));
    last
}

/// Whether the agent's words in `body` say `text`.
fn says(body: &EventBody, text: &str) -> bool {
    let EventBody::Assistant { blocks, .. } = body else { return false };
    blocks
        .iter()
        .any(|block| matches!(block, protocol::event::ContentBlock::Text { text: said, .. } if said.contains(text)))
}

#[gpui_kit::test]
fn the_stop_button_stops_the_turn(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input("say:go; hang");
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
fn empty_prompts_are_not_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
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
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("hello"));
    let transcript = ui.transcript().unwrap();
    let rows = ui.cx.read_entity(&transcript, |t, _| t.row_count());
    let recorded = ui.events().len();
    assert_eq!(rows, recorded, "nothing is left waiting to be recorded");
    assert_eq!(ui.shown("bubble/").len(), 1, "the recorded prompt is drawn once, with no pending copy");
    let prompts = ui.events().iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 1);
}

#[gpui_kit::test]
fn a_turn_says_how_long_its_taken_until_it_ends_and_then_copies_its_reply(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    // A reply that goes on until the test has seen the working line.
    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("slow-until:{done}:word"));
    ui.press("enter");

    ui.wait("the working line", |ui| ui.has("working-line"));
    assert_eq!(ui.session_status(), Some(SessionStatus::Running));
    done.open();
    ui.wait_idle_with("the turn to end", |_| true);
    assert!(!ui.has("working-line"), "it goes with the turn");
    // The session's status and its log arrive separately, in either order.
    let ended = |ui: &Ui| ui.events().iter().find(|e| matches!(e.body, EventBody::TurnEnded { .. })).map(|e| e.seq);
    ui.wait("the end of the turn in the log", |ui| ended(ui).is_some());
    ui.click(gpui_kit::SharedString::from(format!("copy-reply/event/{}/0", ended(&ui).unwrap())));

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("done"), "what the fake agent says its turn came to");
}

/// A code block in a reply has a button at its top right that copies its code, shown while the block is hovered, and
/// pressing it selects nothing.
#[gpui_kit::test]
fn a_code_block_s_copy_button_shows_while_it_s_hovered_and_copies_its_code(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    // The fence is partway along the prompt's line, so only the reply has a code block, which runs to its end.
    ui.paste_text("whole:```sh\nmake trains");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("make trains"));
    let shown = |ui: &mut Ui| match &ui.labelled("Copy code")[..] {
        [copy] => copy.visible(),
        found => panic!("{} buttons to copy code", found.len()),
    };
    let button = ui.labelled("Copy code")[0].bounds();
    let moved = |ui: &mut Ui, position| {
        ui.send(MouseMoveEvent { position, pressed_button: None, modifiers: Modifiers::default() });
    };

    moved(&mut ui, point(px(1.), px(1.)));
    assert!(!shown(&mut ui), "hidden while the block isn't hovered");
    // On the code's first line, to the left of the button and under it.
    moved(&mut ui, button.bottom_left() + point(px(-40.), px(12.)));
    assert!(shown(&mut ui), "shown while it is");
    ui.click_labelled("Copy code", 0);

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("make trains"));
    let transcript = ui.transcript().expect("the chat");
    assert_eq!(ui.cx.read_entity(&transcript, |t, _| t.as_copied()), "", "nothing's selected");
}

/// The `markdown` crate the kit parses with panics on some texts, which the agent can write. A reply recorded with one
/// is drawn as it's written whenever its chat is, rather than ending the app each time.
#[gpui_kit::test]
fn a_reply_the_markdown_parser_panics_on_is_drawn_as_it_is_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let reply = "a\n===\n---\nb\n===";
    stack.run(&session, &format!("say:{reply}"));

    let mut ui = Ui::open(cx, &stack);

    ui.wait("the reply to be drawn", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.kept_texts().iter().any(|text| text == reply)))
    });
}

/// Copy overlaps the prompt's corner without reaching a code-block reply or moving it, and its outer edge copies.
#[gpui_kit::test]
fn a_prompt_s_copy_button_shows_while_it_s_hovered_and_copies_what_it_says(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let prompt = "whole:```python\nprint('Roundhouse')";
    ui.paste_text(prompt);
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("print('Roundhouse')"));
    assert_eq!(ui.labelled("Copy code").len(), 1, "the reply starts with a code block");
    let shown = |ui: &mut Ui| match &ui.labelled("Copy prompt")[..] {
        [copy] => copy.visible(),
        found => panic!("{} buttons to copy a prompt", found.len()),
    };
    let [bubble] = &ui.shown("bubble/")[..] else { panic!("one prompt") };
    let [reply] = &ui.shown("copy-reply/")[..] else { panic!("one finished turn") };
    let [copy] = &ui.shown("copy-prompt/")[..] else { panic!("one button to copy a prompt") };
    let (bubble, reply, copy) = (bubble.clone(), reply.clone(), copy.clone());

    ui.hover(reply.clone());
    assert!(!shown(&mut ui), "hidden while the prompt isn't hovered");
    let (bubble_bounds, reply_bounds, copy_bounds) =
        (ui.bounds(bubble.clone()), ui.bounds(reply.clone()), ui.bounds(copy.clone()));
    let [prompt_row] = &ui.shown("prompt-row/")[..] else { panic!("one prompt row") };
    let row_bottom = ui.bounds(prompt_row.clone()).bottom();
    assert_eq!(row_bottom, bubble_bounds.bottom() + px(16.), "only the usual gap");
    assert!(copy_bounds.bottom() <= row_bottom, "the hit target clears the reply below");
    assert_eq!(copy_bounds.center().y, bubble_bounds.bottom() - px(4.), "one step above the corner");
    assert_eq!(copy_bounds.center().x, bubble_bounds.right(), "centred on its right edge");
    ui.hover(bubble.clone());
    assert!(shown(&mut ui), "shown while it is");
    assert_eq!(ui.bounds(reply.clone()), reply_bounds, "showing it moves nothing below");
    let gap = point(copy_bounds.center().x, bubble_bounds.bottom() + px(1.));
    ui.send(MouseMoveEvent { position: gap, pressed_button: None, modifiers: Modifiers::default() });
    assert!(shown(&mut ui), "stays shown across the corner of the bubble");
    ui.hover(copy.clone());
    assert!(shown(&mut ui), "and while the pointer's on it, outside the bubble");
    let outer_corner = copy_bounds.bottom_right() - point(px(2.), px(2.));
    ui.send(MouseMoveEvent { position: outer_corner, pressed_button: None, modifiers: Modifiers::default() });
    assert!(shown(&mut ui), "including the edge of its hit target");
    assert_eq!(ui.bounds(bubble), bubble_bounds);
    assert_eq!(ui.bounds(copy), copy_bounds);
    assert_eq!(ui.bounds(reply), reply_bounds);
    ui.cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before copy".to_string()));
    ui.click_at(outer_corner);

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some(prompt));

    ui.click("composer");
    ui.paste_text(
        "say:This longer prompt wraps onto several lines so its copy button must keep the same horizontal position \
         as the shorter prompt above it.",
    );
    ui.press("enter");
    ui.wait_idle_with("the longer reply", |text| text.contains("This longer prompt"));
    let buttons = ui.labelled("Copy prompt");
    assert_eq!(buttons.len(), 2);
    assert_eq!(buttons[0].bounds().right(), buttons[1].bounds().right(), "different lengths keep Copy aligned");
}

/// Inline code that's one word too long for a line wraps across the bubble's width rather than early, and the bubble's
/// height fits every wrapped line (`#1200`).
#[gpui_kit::test]
fn a_prompt_with_inline_code_that_is_one_long_word_fills_and_stays_inside_its_bubble(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let token = "0123456789abcdefghijklmnopqrstuvwxyz".repeat(4);
    let prompt = format!("say:`{token}`");
    ui.paste_text(&prompt);
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains(&token));

    let [bubble] = &ui.shown("bubble/")[..] else { panic!("one prompt") };
    let [prompt_row] = &ui.shown("prompt-row/")[..] else { panic!("one prompt row") };
    let (bubble_bounds, row_bounds) = (ui.bounds(bubble.clone()), ui.bounds(prompt_row.clone()));
    // Prompts cap at 80% of the column; an unpadded per-grapheme wrap shrank this to ~42%.
    assert!(
        bubble_bounds.size.width > row_bounds.size.width * 0.7,
        "the wrapped inline code uses the bubble's width: {bubble_bounds:?} in {row_bounds:?}"
    );
    let transcript = ui.transcript().unwrap();
    let leaves = ui.cx.read_entity(&transcript, |t, _| t.drawn_leaf_bounds());
    let prompt_leaf = leaves[0];
    assert!(
        prompt_leaf.top() >= bubble_bounds.top() && prompt_leaf.bottom() <= bubble_bounds.bottom(),
        "every line of the inline code stays inside the bubble: {prompt_leaf:?} vs {bubble_bounds:?}"
    );
}
