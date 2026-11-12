//! Queueing: Cmd-Enter keeps what's typed for after the agent finishes, and the queue above the composer changes it.

#![cfg(test)]

use gpui_kit::{
    AppContext as _, ClipboardEntry, ClipboardItem, ElementId, ExternalPaths, Image, ImageFormat, ScrollDelta,
    SharedString, TestAppContext, point, px,
};
use protocol::event::EventBody;
use protocol::model::QueuedPrompt;

use crate::steering::working;
use crate::support::{Stack, Ui};

pub(crate) fn queued(ui: &Ui) -> Vec<QueuedPrompt> {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, |p, _| p.session().map(|s| s.queued.clone()).unwrap_or_default())
}

fn held(ui: &Ui) -> bool {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, |p, _| p.session().is_some_and(|s| s.queue_held))
}

fn id(name: &str, prompt: &QueuedPrompt) -> ElementId {
    ElementId::Name(SharedString::from(format!("{name}-{}", prompt.client_msg_id)))
}

#[gpui_kit::test]
fn cmd_enter_queues_while_the_agent_works_and_the_hub_sends_it_once_the_turn_ends(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 1500);

    ui.input("say:from the queue");
    ui.press("secondary-enter");

    assert_eq!(ui.composer_text(), "");
    ui.wait("the queue to show it", |ui| queued(ui).len() == 1 && ui.visible("queue"));
    ui.wait_idle_with("the queued prompt's answer", |text| text.contains("from the queue"));
    let ends = ui.events().iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    assert_eq!(ends, 2, "a turn of its own");
    ui.wait("the queue to go", |ui| queued(ui).is_empty() && !ui.has("queue"));
}

#[gpui_kit::test]
fn cmd_enter_with_the_agent_idle_just_sends(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();

    ui.input("say:now");
    ui.press("secondary-enter");

    ui.wait_idle_with("the answer", |text| text.contains("now"));
    assert!(queued(&ui).is_empty());
}

#[gpui_kit::test]
fn a_queued_prompt_can_be_changed_or_taken_out_before_its_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 4000);
    ui.input("say:kept");
    ui.press("secondary-enter");
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
        queue.len() == 1 && queue[0].client_msg_id == first.client_msg_id && queue[0].text == "say:changed"
    });
    ui.wait_idle_with("the changed prompt's answer", |text| text.contains("changed"));
    assert!(!ui.assistant_text().contains("kept") && !ui.assistant_text().contains("taken out"));
}

#[gpui_kit::test]
fn escape_leaves_a_queued_prompt_as_it_was(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 4000);
    ui.input("say:as it was");
    ui.press("secondary-enter");
    ui.wait("it to be queued", |ui| queued(ui).len() == 1);
    let prompt = queued(&ui)[0].clone();

    ui.hover(id("queued", &prompt));
    ui.click(id("edit-queued", &prompt));
    ui.input(" and then some");
    ui.press("escape");

    assert!(ui.has(id("queued", &prompt)), "the text shows again, not the editor");
    ui.input("typed after");
    assert_eq!(ui.composer_text(), "typed after", "back in the composer");
    assert_eq!(queued(&ui)[0].text, "say:as it was");
}

#[gpui_kit::test]
fn a_queue_held_by_a_stop_waits_until_a_prompt_in_it_is_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 20000);
    ui.input("say:after the stop");
    ui.press("secondary-enter");
    ui.wait("it to be queued", |ui| queued(ui).len() == 1);

    ui.click("stop");

    let prompt = queued(&ui)[0].clone();
    ui.wait("the queue to wait", |ui| held(ui));
    assert!(!ui.visible(id("send-queued", &prompt)), "only with the pointer over it, even held");
    ui.hover(id("queued", &prompt));
    ui.click(id("send-queued", &prompt));
    ui.wait_idle_with("the queued prompt's answer", |text| text.contains("after the stop"));
    ui.wait("the queue to go", |ui| queued(ui).is_empty() && !ui.has(id("send-queued", &prompt)));
}

#[gpui_kit::test]
fn a_long_queue_scrolls_rather_than_pushing_the_composer_out_of_the_window(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 20000);
    let pane = ui.pane().unwrap();
    let session_id = ui.cx.read_entity(&pane, |p, _| p.session().map(|s| s.id.clone())).unwrap();
    let client = stack.client();
    for i in 0..30 {
        client.session_queue(&session_id, format!("q{i}").into(), &format!("say:queued {i}")).unwrap();
    }

    ui.wait("the whole queue", |ui| queued(ui).len() == 30);

    assert!(ui.visible("stop"), "the composer's controls stay in the window");
    let last = queued(&ui)[29].clone();
    assert!(!ui.visible(id("queued", &last)), "the queue keeps to its height");
    ui.scroll(id("queued", &queued(&ui)[0]), ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    assert!(ui.visible(id("queued", &last)), "and scrolls to the rest");
    assert!(ui.visible("stop"));
}

#[gpui_kit::test]
fn the_arrow_on_a_queued_prompt_steers_the_agent_with_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 3000);
    ui.input("say:steered from the queue");
    ui.press("secondary-enter");
    ui.wait("it to be queued", |ui| queued(ui).len() == 1);
    let prompt = queued(&ui)[0].clone();
    assert!(!ui.visible(id("send-queued", &prompt)), "only with the pointer over it");

    ui.hover(id("queued", &prompt));
    ui.click(id("send-queued", &prompt));

    ui.wait("it to leave the queue", |ui| queued(ui).is_empty());
    ui.wait_idle_with("the steered answer", |text| text.contains("steered from the queue"));
    // The record can say the agent is idle before the chat has the event that ended its turn, since the two come by
    // different subscriptions.
    ui.wait("the turn to end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));
    let events = ui.events();
    let at = |found: fn(&EventBody) -> bool| events.iter().position(|e| found(&e.body)).unwrap();
    let sent = at(|body| matches!(body, EventBody::UserPrompt { text, .. } if text.contains("steered")));
    let ended = at(|body| matches!(body, EventBody::TurnEnded { .. }));
    assert!(sent < ended, "into the running turn, not one after it");
}

#[gpui_kit::test]
fn cmd_enter_queues_a_prompt_with_its_files_and_the_agent_gets_them_with_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 4000);
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 3, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    let len = png.len();
    ui.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Png, png)));
    ui.wait("the image to be attached", |ui| ui.attached() == ["Pasted image.png"]);
    ui.input("attached");

    ui.press("secondary-enter");

    assert_eq!(ui.composer_text(), "");
    assert!(ui.attached().is_empty(), "queueing takes what was attached");
    ui.wait("the queue to show it with its file", |ui| {
        let queue = queued(ui);
        queue.first().is_some_and(|prompt| prompt.attachments.len() == 1 && ui.visible(id("queued-files", prompt)))
    });
    let said = format!("attached: image/png {len} bytes | Pasted image.png: on disk, {len} bytes");
    ui.wait_idle_with("the agent to get the image", |text| text.ends_with(&said));
    ui.wait("the queue to go", |ui| queued(ui).is_empty() && !ui.has("queue"));
}

#[gpui_kit::test]
fn a_prompt_shows_in_the_queue_while_its_files_are_added(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    working(&mut ui, 20000);
    let dir = tempfile::tempdir().unwrap();
    // Big enough to take a while to add, so the hub doesn't have the prompt yet when the queue's looked at.
    let big = dir.path().join("big.log");
    std::fs::write(&big, vec![b'a'; 40 << 20]).unwrap();
    ui.paste(ClipboardItem::from(ClipboardEntry::ExternalPaths(ExternalPaths(vec![big].into()))));
    ui.wait("the file to be attached", |ui| ui.attached() == ["big.log"]);
    ui.input("say:with a big file");

    ui.press("secondary-enter");

    assert!(ui.visible("queue") && queued(&ui).is_empty(), "it shows before the hub has it");
    ui.wait("the hub to have it", |ui| queued(ui).len() == 1);
    let prompt = queued(&ui)[0].clone();
    assert!(ui.visible(id("queued-files", &prompt)));
    assert_eq!(ui.shown("queued-files-").len(), 1, "it shows once, not again as pending");
}
