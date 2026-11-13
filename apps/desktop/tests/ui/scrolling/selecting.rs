//! Selecting what's written in a chat: by dragging, past the window's edge too, which scrolls the transcript on for as
//! long as the pointer is held out there, either way, and keeps what's selected, and over a link, which a drag doesn't
//! open; a word or a paragraph at a time; or all of it.

use gpui_kit::base::TextSelection;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, ClipboardItem, ElementId, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, ScrollDelta, TestAppContext, point, px,
};
use protocol::event::EventBody;
use protocol::model::{SessionId, seq_index};

use super::{A_FRAME, press, wait_for_the_list_to_settle};
use crate::support::{Stack, Ui};

mod calls;
mod kinds;
mod tab;

/// A reply the tests find by what it says.
const LAST: &str = "Signal boxes keep ledgers, and the lever frame holds each route clear.";

/// How many frames the pointer is held past the bottom edge, and then twice that past the top, to go back past where
/// it started.
const HELD: u16 = 60;

#[gpui_kit::test]
fn dragging_a_selection_past_the_windows_edge_scrolls_on_either_way_and_keeps_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    // Short prompts, each answered by a run of replies of a few paragraphs.
    for prompt in 0..8 {
        let replies: Vec<String> = (0..25).map(|reply| format!("prose:3:{}", prompt * 25 + reply)).collect();
        stack.run(&session, &replies.join("; "));
    }
    let mut ui = open(cx, &stack);
    let window = settled(&mut ui, &stack, &session);
    // Some way up from the end, so there's room to scroll either way.
    for _ in 0..20 {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    }
    ui.settle(A_FRAME);
    let start = a_reply(&ui, window);
    let held = Some(MouseButton::Left);
    press(&mut ui, MouseDownEvent { button: MouseButton::Left, position: start, click_count: 1, ..Default::default() });
    press(
        &mut ui,
        MouseMoveEvent { position: point(start.x, start.y + px(20.)), pressed_button: held, ..Default::default() },
    );
    let first = copy(&mut ui);

    let below = point(start.x, window.bottom() + px(40.));
    press(&mut ui, MouseMoveEvent { position: below, pressed_button: held, ..Default::default() });
    let going_down = hold(&mut ui, HELD.into());
    let down = copy(&mut ui);
    let above = point(start.x, window.top() - px(40.));
    press(&mut ui, MouseMoveEvent { position: above, pressed_button: held, ..Default::default() });
    let going_up = hold(&mut ui, (2 * HELD).into());
    let up = copy(&mut ui);
    press(&mut ui, MouseUpEvent { button: MouseButton::Left, position: above, click_count: 1, ..Default::default() });
    let released = copy(&mut ui);
    ui.press("secondary-a");
    let all = copy(&mut ui);

    let moved = |steps: &[Pixels]| steps.last().copied().unwrap_or_default() - steps[0];
    let (moved_down, moved_up) = (moved(&going_down), moved(&going_up));
    assert!(!first.is_empty(), "a drag within the window selects");
    assert!(moved_down > px(f32::from(HELD) * 12.), "held below, it went down {moved_down:?}");
    // By then the row it started in is well above the window, and isn't drawn.
    assert!(down.starts_with(&first), "what's selected still starts where it did: {:?}", head(&down));
    assert!(down.len() > first.len() + 1000, "and goes on past what the window held: {} bytes", down.len());
    assert!(moved_up < -(moved_down + px(500.)), "held above, it went back up past where it started: {moved_up:?}");
    assert!(!up.contains(&first), "what's selected now ends where it started: {:?}", head(&up));
    assert!(all.contains(&format!("{up}{down}")), "just where the drag down began");
    assert!(up.len() > 1000, "and starts well above it: {} bytes", up.len());
    assert_eq!(released, up, "and it stays selected once the button's up");
}

/// A drag in a chat that stops showing, such as for another chat, is over, as the button can come up where the chat
/// can't hear it. So with the pointer left past the window's edge, the chat doesn't scroll on once it's back.
#[gpui_kit::test]
fn a_drag_is_over_once_its_chat_has_not_shown(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let replies: Vec<String> = (0..25).map(|reply| format!("prose:3:{reply}")).collect();
    stack.run(&session, &replies.join("; "));
    let mut ui = open(cx, &stack);
    let other = ui.new_chat();
    ui.activate_chat(&session);
    let window = settled(&mut ui, &stack, &session);
    for _ in 0..10 {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(300.))));
    }
    ui.settle(A_FRAME);
    let start = a_reply(&ui, window);
    let held = Some(MouseButton::Left);
    press(&mut ui, MouseDownEvent { button: MouseButton::Left, position: start, click_count: 1, ..Default::default() });
    let below = point(start.x, window.bottom() + px(40.));
    press(&mut ui, MouseMoveEvent { position: below, pressed_button: held, ..Default::default() });
    let before = hold(&mut ui, 5);

    ui.activate_chat(&other);
    ui.play(A_FRAME);
    ui.activate_chat(&session);
    let back = hold(&mut ui, 30);

    let moved = |steps: &[Pixels]| steps.last().copied().unwrap_or_default() - steps[0];
    assert!(moved(&before) > px(10.), "the drag was scrolling the chat before: {:?}", moved(&before));
    assert!(moved(&back).abs() < px(1.), "and once it was back, it stayed where it was: {:?}", moved(&back));
}

/// A drag across the agent's words is the transcript's selection alone. The kit's, which still selects what a call
/// shows, starts from the nearest of its own texts to a press, and would select what of its texts a drag goes across,
/// such as an opened call's command and what it printed.
#[gpui_kit::test]
fn a_drag_across_the_agent_s_words_selects_nothing_of_the_kit_s(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let (first, second) = ("Signal boxes keep ledgers of every train.", "The lever frame holds each route clear.");
    stack.run(&session, &format!("say:{first}; tool:echo the-points-are-set; say:{second}"));
    let mut ui = open(cx, &stack);
    settled(&mut ui, &stack, &session);
    ui.click_labelled("Run", 0);
    let window = settled(&mut ui, &stack, &session);
    let (from, to) = (on_the_reply_saying(&ui, window, first), on_the_reply_saying(&ui, window, second));

    let (button, held) = (MouseButton::Left, Some(MouseButton::Left));
    press(&mut ui, MouseDownEvent { button, position: from, click_count: 1, ..Default::default() });
    // Across all of the call between, wherever its words are drawn, and on into the next reply.
    let mut y = from.y;
    while y < to.y {
        for x in [window.left() + px(40.), window.center().x, window.right() - px(60.)] {
            press(&mut ui, MouseMoveEvent { position: point(x, y), pressed_button: held, ..Default::default() });
        }
        y += px(6.);
    }
    let to = point(to.x + px(100.), to.y);
    press(&mut ui, MouseMoveEvent { position: to, pressed_button: held, ..Default::default() });
    let kit_s = ui.cx.update_window(ui.handle, |_, window, cx| TextSelection::has_selection(window, cx)).unwrap();
    press(&mut ui, MouseUpEvent { button, position: to, click_count: 1, ..Default::default() });
    let copied = copy(&mut ui);

    assert!(copied.contains("ledgers") && copied.contains("lever"), "the drag selected from one reply into the next");
    assert!(!kit_s, "and the kit selected nothing, of the call between them or anything else");
}

/// A press on the scrollbar scrolls, and leaves what's selected as it was, so the far end of it can be scrolled to.
#[gpui_kit::test]
fn a_press_on_the_scrollbar_keeps_what_s_selected(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, &format!("prose:12:1; prose:12:2; say:{LAST}"));
    let mut ui = open(cx, &stack);
    let window = settled(&mut ui, &stack, &session);
    let on_it = on_the_reply_saying(&ui, window, LAST);
    click(&mut ui, on_it, 2);
    let word = copy(&mut ui);

    click(&mut ui, point(window.right() - px(8.), window.bottom() - px(30.)), 1);

    assert!(!word.is_empty(), "a word was selected");
    assert_eq!(copy(&mut ui), word, "and still is");
}

/// A double click selects the word it's on, a triple click its paragraph, and Cmd-A, in a chat a press has given
/// focus, all of the words the chat has, the person's and the agent's, in order.
#[gpui_kit::test]
fn a_word_a_paragraph_or_everything_written_can_be_selected(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let prompt = format!("say:The first reply; prose:3:1; say:{LAST}");
    stack.run(&session, &prompt);
    let mut ui = open(cx, &stack);
    let window = settled(&mut ui, &stack, &session);
    let on_it = on_the_reply_saying(&ui, window, LAST);

    click(&mut ui, on_it, 2);
    let word = copy(&mut ui);
    click(&mut ui, on_it, 3);
    let paragraph = copy(&mut ui);
    let all = everything(&mut ui, on_it);

    assert!(!word.is_empty() && !word.contains(' ') && LAST.contains(&word), "a word of it: {word:?}");
    assert_eq!(paragraph, LAST, "all of its paragraph");
    let first = format!("{prompt}\n\nThe first reply\n\n");
    assert!(all.starts_with(&first), "the prompt, then every reply, from the first: {:?}", head(&all));
    assert!(all.ends_with(LAST), "to the last");
}

/// A shift-click selects from where a press started what's selected to where it's clicked.
#[gpui_kit::test]
fn a_shift_click_selects_from_where_a_press_was_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let (first, second) = ("Signal boxes keep ledgers of every train.", "The lever frame holds each route clear.");
    stack.run(&session, &format!("say:{first}; prose:3:1; say:{second}"));
    let mut ui = open(cx, &stack);
    let window = settled(&mut ui, &stack, &session);
    let (from, to) = (on_the_reply_saying(&ui, window, first), on_the_reply_saying(&ui, window, second));
    let (button, modifiers) = (MouseButton::Left, Modifiers { shift: true, ..Default::default() });

    click(&mut ui, from, 1);
    let pressed = copy(&mut ui);
    press(&mut ui, MouseDownEvent { button, position: to, click_count: 1, modifiers, ..Default::default() });
    press(&mut ui, MouseUpEvent { button, position: to, click_count: 1, modifiers });
    let extended = copy(&mut ui);

    assert_eq!(pressed, "", "a click selects nothing");
    let (start, end) = (extended.split("\n\n").next().unwrap(), extended.rsplit("\n\n").next().unwrap());
    assert!(first.ends_with(start) && start.len() < first.len(), "from where it was: {:?}", head(&extended));
    assert!(second.starts_with(end) && !end.is_empty(), "to where the shift-click was: {:?}", tail(&extended));
    assert!(extended.split("\n\n").count() > 2, "with what's between");
}

/// A click on a link opens it, but a drag that selects some of a reply and ends on a link doesn't.
#[gpui_kit::test]
fn a_drag_that_ends_on_a_link_selects_rather_than_opening_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let reply = "[Signal boxes](https://example.com/boxes) keep ledgers, and the lever frame holds each route clear.";
    stack.run(&session, &format!("say:{reply}"));
    let mut ui = open(cx, &stack);
    let window = settled(&mut ui, &stack, &session);
    let on_the_link = on_the_reply_saying(&ui, window, reply);
    let along = point(on_the_link.x + px(200.), on_the_link.y);

    let (button, held) = (MouseButton::Left, Some(MouseButton::Left));
    press(&mut ui, MouseDownEvent { button, position: along, click_count: 1, ..Default::default() });
    press(&mut ui, MouseMoveEvent { position: on_the_link, pressed_button: held, ..Default::default() });
    press(&mut ui, MouseUpEvent { button, position: on_the_link, click_count: 1, ..Default::default() });
    let dragged = ui.cx.opened_url();
    let selected = copy(&mut ui);
    click(&mut ui, on_the_link, 1);
    let clicked = ui.cx.opened_url();

    assert_eq!(dragged, None, "the drag opened nothing");
    assert!(selected.contains("boxes keep"), "it selected from the link on: {selected:?}");
    assert_eq!(clicked.as_deref(), Some("https://example.com/boxes"), "a click opens it");
}

/// A link that names a definition elsewhere in the text goes where the definition says, as in the kit's text view.
#[gpui_kit::test]
fn a_reference_link_opens_where_its_definition_says(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    let reply = "[Signal boxes][boxes] keep ledgers.\n\n[boxes]: https://example.com/boxes";
    stack.run(&session, &format!("say:{reply}"));
    let mut ui = open(cx, &stack);
    let window = settled(&mut ui, &stack, &session);

    let on_the_link = on_the_reply_saying(&ui, window, reply);

    click(&mut ui, on_the_link, 1);

    assert_eq!(ui.cx.opened_url().as_deref(), Some("https://example.com/boxes"));
}

/// The app, open on the stack's chat.
fn open<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    ui
}

/// Where the transcript is, once its list has settled at the end of `session`.
fn settled(ui: &mut Ui<'_>, stack: &Stack, session: &SessionId) -> Bounds<Pixels> {
    wait_for_the_list_to_settle(ui, seq_index(stack.client().session_get(session).unwrap().last_seq) - 1);
    ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap()
}

/// Where the element `id` was drawn last.
fn bounds(ui: &mut Ui<'_>, id: ElementId) -> Bounds<Pixels> {
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
}

/// Presses the button at `at`, and lets it go, as the `clicks`th click in a row.
fn click(ui: &mut Ui<'_>, at: gpui_kit::Point<Pixels>, clicks: usize) {
    let button = MouseButton::Left;
    press(ui, MouseDownEvent { button, position: at, click_count: clicks, ..Default::default() });
    press(ui, MouseUpEvent { button, position: at, click_count: clicks, ..Default::default() });
}

/// A point on the first line of the reply that says `text`, which is in the window.
fn on_the_reply_saying(ui: &Ui<'_>, window: Bounds<Pixels>, text: &str) -> gpui_kit::Point<Pixels> {
    let transcript = ui.transcript().unwrap();
    ui.cx.read_entity(&transcript, |t, _| {
        let list = t.list();
        let top = list.top();
        let mut y = window.top() - top.offset;
        for row in top.row..list.row_count() {
            let says = t.event_at(row).is_some_and(|event| match event.body {
                EventBody::Assistant { blocks, .. } => blocks.iter().any(
                    |block| matches!(block, protocol::event::ContentBlock::Text { text: said, .. } if said == text),
                ),
                _ => false,
            });
            if says {
                return point(window.left() + px(40.), y + px(12.));
            }
            y += list.row_height(row).unwrap_or_default();
        }
        panic!("no reply saying {text:?} in the window")
    })
}

/// What's selected, as a copy has it, with the keys the kit copies with where the tests run. The clipboard's emptied
/// first, so a copy of nothing doesn't read as the last.
fn copy(ui: &mut Ui<'_>) -> String {
    ui.cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
    ui.press("secondary-c");
    ui.cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default()
}

/// Everything Cmd-A selects, as a copy has it, with the chat given the focus by a click at `at`, on one of its texts.
fn everything(ui: &mut Ui<'_>, at: gpui_kit::Point<Pixels>) -> String {
    click(ui, at, 1);
    ui.press("secondary-a");
    copy(ui)
}

/// The start of `text`, to say what a copy held.
fn head(text: &str) -> String {
    text.chars().take(80).collect()
}

/// The end of `text`, to say what a copy held.
fn tail(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars[chars.len().saturating_sub(80)..].iter().collect()
}

/// A point on the text of a reply in the upper half of the window, by the heights of the rows above it.
fn a_reply(ui: &Ui<'_>, window: Bounds<Pixels>) -> gpui_kit::Point<Pixels> {
    let transcript = ui.transcript().unwrap();
    ui.cx.read_entity(&transcript, |t, _| {
        let list = t.list();
        let top = list.top();
        let mut y = window.top() - top.offset;
        for row in top.row..list.row_count() {
            let height = list.row_height(row).unwrap_or_default();
            let reply = t.event_at(row).is_some_and(|event| matches!(event.body, EventBody::Assistant { .. }));
            if reply && height > px(60.) && y > window.top() + px(20.) && y < window.center().y {
                return point(window.left() + px(40.), y + px(12.));
            }
            y += height;
        }
        panic!("no reply in the upper half of the window")
    })
}

/// Holds the pointer where it is for `frames` frames, and says how far down the session the window was after each.
fn hold(ui: &mut Ui<'_>, frames: usize) -> Vec<Pixels> {
    let transcript = ui.transcript().unwrap();
    let down = |ui: &Ui<'_>| {
        ui.cx.read_entity(&transcript, |t, _| {
            let list = t.list();
            let top = list.top();
            let above: Pixels = (0..top.row).map(|row| list.row_height(row).unwrap_or_default()).sum();
            above + top.offset
        })
    };
    let mut steps = vec![down(ui)];
    for _ in 0..frames {
        ui.play(A_FRAME);
        steps.push(down(ui));
    }
    steps
}
