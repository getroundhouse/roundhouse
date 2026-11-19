//! Queueing: Cmd-Enter keeps what's typed for after the agent finishes, and the queue above the composer changes it.
//! When the hub sends what's queued is the hub's, tested in `crates/hub/src/hub/queues/tests/`, so each of its rules
//! here is an example of the queue's buttons and keys reaching it.

#![cfg(test)]

use client_core::fake_hub::events::{taken, user_prompt};
use gpui_kit::{
    AppContext as _, ClipboardItem, ElementId, Image, ImageFormat, ScrollDelta, SharedString, TestAppContext, point, px,
};
use protocol::event::EventBody;
use protocol::model::{QueuedPrompt, Seq, SessionStatus};
use protocol::prompt::Prompt;

use crate::steering::working;
use crate::support::{Frozen, Gate, Stack, Ui, one_chat, open_on_the_chat};
use protocol::prompt::PromptFrom;

pub(crate) fn queued(ui: &Ui) -> Vec<QueuedPrompt> {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, |p, _| p.session().map_or_default(|s| s.queued.clone()))
}

fn held(ui: &Ui) -> bool {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, |p, _| p.session().is_some_and(|s| s.queue_held))
}

fn id(name: &str, prompt: &QueuedPrompt) -> ElementId {
    ElementId::Name(SharedString::from(format!("{name}-{}", prompt.client_msg_id)))
}

/// A window with a new workspace whose agent is at work until the gate it returns is opened.
fn at_work<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, Gate) {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    let done = working(&mut ui, stack);
    (ui, done)
}

/// What's queued can be changed or taken out before it's sent, and Escape leaves it as it was, and the hub sends what's
/// left as a turn of its own once the agent's turn ends.
#[gpui_kit::test]
fn cmd_enter_queues_what_can_be_changed_or_taken_out_until_the_hub_sends_it_once_the_turn_ends(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, done) = at_work(cx, &stack);

    ui.input("say:kept");
    ui.press("secondary-enter");

    assert_eq!(ui.composer_text(), "");
    ui.wait("the queue to show it", |ui| queued(ui).len() == 1 && ui.visible("queue"));
    ui.input("say:taken out");
    ui.press("secondary-enter");
    ui.wait("both in the queue", |ui| queued(ui).len() == 2);
    let (first, second) = (queued(&ui)[0].clone(), queued(&ui)[1].clone());

    ui.hover(id("queued", &first));
    ui.click(id("edit-queued", &first));
    ui.press("secondary-a");
    ui.input("say:changed");
    ui.press("enter");
    ui.hover(id("queued", &second));
    ui.click(id("remove-queued", &second));

    ui.wait("the change and the removal", |ui| {
        let queue = queued(ui);
        queue.len() == 1 && queue[0].client_msg_id == first.client_msg_id && queue[0].prompt.text == "say:changed"
    });
    ui.hover(id("queued", &first));
    ui.click(id("edit-queued", &first));
    ui.input(" and then some");
    ui.press("escape");
    assert!(ui.has(id("queued", &first)), "after Escape, the text shows again, not the editor");
    ui.input("typed after");
    assert_eq!(ui.composer_text(), "typed after", "back in the composer");
    assert_eq!(queued(&ui)[0].prompt.text, "say:changed", "and the queued prompt is as it was");

    done.open();
    ui.wait_idle_with("the changed prompt's answer", |text| text.contains("changed"));
    assert!(!ui.assistant_text().contains("kept") && !ui.assistant_text().contains("taken out"));
    let ends = ui.events().iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    assert_eq!(ends, 2, "a turn of its own");
    ui.wait("the queue to go", |ui| queued(ui).is_empty() && !ui.has("queue"));
}

/// The arrow, which shows only with the pointer over its prompt, sends it into the turn the agent is on, and once a
/// stop has held the queue, as a turn of its own.
#[gpui_kit::test]
fn the_arrow_on_a_queued_prompt_steers_the_agent_with_it_or_sends_it_once_a_stop_has_held_the_queue(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, done) = at_work(cx, &stack);
    ui.input("say:steered from the queue");
    ui.press("secondary-enter");
    ui.wait("it to be queued", |ui| queued(ui).len() == 1);
    let prompt = queued(&ui)[0].clone();
    assert!(!ui.visible(id("send-queued", &prompt)), "only with the pointer over it");

    ui.hover(id("queued", &prompt));
    ui.click(id("send-queued", &prompt));

    ui.wait("it to leave the queue", |ui| queued(ui).is_empty());
    done.open_once_the_agent_has_a_prompt();
    ui.wait_idle_with("the steered answer", |text| text.contains("steered from the queue"));
    // The record can say the agent is idle before the chat has the event that ended its turn, since the two come by
    // different subscriptions.
    ui.wait("the turn to end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));
    let events = ui.events();
    let at = |found: fn(&EventBody) -> bool| events.iter().position(|e| found(&e.body)).unwrap();
    let sent = at(
        |body| matches!(body, EventBody::UserPrompt { prompt: Prompt { text, .. }, .. } if text.contains("steered")),
    );
    let ended = at(|body| matches!(body, EventBody::TurnEnded { .. }));
    assert!(sent < ended, "into the running turn, not one after it");

    let stopped = Gate::new(&stack, "stopped-done");
    ui.paste_text(&format!("hang-until:{stopped}"));
    ui.press("enter");
    ui.wait("the agent to start the turn to stop", |ui| ui.session_status() == Some(SessionStatus::Running));
    ui.input("say:after the stop");
    ui.press("secondary-enter");
    ui.wait("the prompt after the stop to be queued", |ui| queued(ui).len() == 1);
    ui.click("stop");
    let prompt = queued(&ui)[0].clone();
    ui.wait("the queue to wait", |ui| held(ui));
    assert!(!ui.visible(id("send-queued", &prompt)), "only with the pointer over it, even held");
    ui.hover(id("queued", &prompt));
    ui.click(id("send-queued", &prompt));
    ui.wait_idle_with("the held prompt's answer", |text| text.contains("after the stop"));
    ui.wait("the queue to go", |ui| queued(ui).is_empty() && !ui.has(id("send-queued", &prompt)));
}

/// Queueing takes what's attached with what's typed, and the prompt shows in the queue while its files go to the hub,
/// once, and the agent gets them with it.
#[gpui_kit::test]
fn cmd_enter_queues_a_prompt_with_its_files_which_shows_while_they_re_added_and_the_agent_gets_them(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, done) = at_work(cx, &stack);
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 3, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    let len = png.len();
    // Hold the upload until the pending row is checked, independently of file size or machine load.
    let frozen = Frozen::stop(stack.client().hub_status().unwrap().pid);
    ui.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Png, png)));
    ui.wait("the image to be attached", |ui| ui.attached() == ["Pasted image.png"]);
    ui.input("attached");

    ui.press("secondary-enter");

    assert_eq!(ui.composer_text(), "");
    assert!(ui.attached().is_empty(), "queueing takes what was attached");
    assert!(ui.visible("queue") && queued(&ui).is_empty(), "it shows before the hub has it");
    drop(frozen);
    ui.wait("the queue to show it with its file, from the hub", |ui| {
        let queue = queued(ui);
        queue.first().is_some_and(|prompt| prompt.prompt.files.len() == 1 && ui.visible(id("queued-files", prompt)))
    });
    assert_eq!(ui.shown("queued-files-").len(), 1, "it shows once, not again as pending");
    done.open();
    let said = format!("attached: image/png {len} bytes | Pasted image.png: on disk, {len} bytes");
    ui.wait_idle_with("the agent to get the image", |text| text.ends_with(&said));
    ui.wait("the queue to go", |ui| queued(ui).is_empty() && !ui.has("queue"));
}

#[gpui_kit::test]
fn a_long_queue_scrolls_rather_than_pushing_the_composer_out_of_the_window(cx: &mut TestAppContext) {
    let (mut world, chat) = one_chat(SessionStatus::Running, vec![user_prompt("Work on it."), taken(Seq::new(1))]);
    world.sessions.last_mut().unwrap().queued = Vec::from_fn(30, |i| QueuedPrompt {
        client_msg_id: format!("q{i}").into(),
        prompt: Prompt::new(format!("say:queued {i}")),
        from: PromptFrom::Person,
    });
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);

    ui.wait("the whole queue", |ui| queued(ui).len() == 30 && ui.visible("queue"));

    assert!(ui.visible("stop"), "the composer's controls stay in the window");
    let last = queued(&ui)[29].clone();
    assert!(!ui.visible(id("queued", &last)), "the queue keeps to its height");
    ui.scroll(id("queued", &queued(&ui)[0]), ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    assert!(ui.visible(id("queued", &last)), "and scrolls to the rest");
    assert!(ui.visible("stop"));
}
