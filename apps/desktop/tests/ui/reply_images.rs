//! Images a reply points to in the worktree. The worker keeps a copy of each with the reply's record, so how the window
//! draws and opens those is tested on a fake hub holding a made-up reply and its copies, with no worker or agent to
//! wait for. Only a copy made at the time, and a reply drawn while it's written, need the real ones.

#![cfg(test)]

use std::io::Cursor;
use std::path::Path;

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, first_turn, text_pointing, typed_call, user_prompt};
use gpui_kit::{AppContext as _, Pixels, TestAppContext, px};
use protocol::attachment::{Attachment, MediaType};
use protocol::capability::calls::Call;
use protocol::event::{ContentBlock, EventBody, LinkedImage};
use protocol::fixtures::{ApprovalRequest, approval_request, plan};
use protocol::model::{AttachmentId, SessionStatus, ToolUseId};

use crate::support::{FakeHub, Gate, Stack, Ui};

/// A PNG `height` pixels tall, so a reply that shows it is at least that tall.
fn png(height: u32) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    let image = image::RgbaImage::from_pixel(4, height, image::Rgba([200, 30, 30, 255]));
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    png.into_inner()
}

/// Writes a PNG `height` pixels tall at `path`.
fn write_png(path: &Path, height: u32) {
    std::fs::write(path, png(height)).unwrap();
}

/// How tall each reply that points to an image is drawn, in order.
fn replies(ui: &mut Ui<'_>) -> Vec<Pixels> {
    ui.frame();
    let transcript = ui.transcript().unwrap();
    ui.cx.read_entity(&transcript, |t, _| {
        let points = |row: &usize| {
            t.event_at(*row).is_some_and(|event| match event.body {
                EventBody::Assistant { blocks, .. } => {
                    let draws = |text: &str| text.contains("![") || text.contains("```mermaid");
                    blocks.iter().any(|block| matches!(block, ContentBlock::Text { text, .. } if draws(text)))
                }
                _ => false,
            })
        };
        (0..t.row_count()).filter(points).filter_map(|row| t.list().row_height(row)).collect()
    })
}

/// The window on a fake hub holding a chat whose one reply says `reply`, with the copy the hub kept of each PNG in
/// `kept`, by where the reply says it is and how tall it is, as the worker names one for each place the reply points
/// to it. The hub goes with the test, so it's returned to be kept.
fn replied<'a>(cx: &'a mut TestAppContext, reply: &str, kept: &[(&str, u32)]) -> (FakeHub, Ui<'a>) {
    let kept: Vec<_> = kept.iter().map(|(address, height)| (*address, MediaType::Png, png(*height))).collect();
    replied_with(cx, reply, &kept)
}

/// The same, with each image in `kept` as its kind and bytes.
fn replied_with<'a>(cx: &'a mut TestAppContext, reply: &str, kept: &[(&str, MediaType, Vec<u8>)]) -> (FakeHub, Ui<'a>) {
    let mut world = World::default();
    let mut images = Vec::new();
    for (address, media_type, data) in kept {
        let data = data.clone();
        let id = AttachmentId::from(format!("kept-{address}"));
        let attachment = Attachment {
            id: id.clone(),
            name: (*address).to_string(),
            media_type: *media_type,
            size_bytes: u64::try_from(data.len()).unwrap(),
        };
        let places = reply.matches(&format!("]({address})")).count();
        images.extend(std::iter::repeat_n(LinkedImage { address: (*address).to_string(), attachment }, places));
        world.attachments.insert(id, data);
    }
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let events = first_turn(user_prompt("Show me."), [text_pointing("shown", reply, images), finished_turn(3, 0.01)]);
    world.chat(&workspace, SessionStatus::Idle, events);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    (fake, ui)
}

/// The worker reads the file as the reply is recorded, so each reply keeps what it was then.
#[gpui_kit::test]
fn an_image_pointed_to_again_after_it_changed_shows_as_it_is_then(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    let shot = ui.worktree(&workspace).join("shot.png");
    write_png(&shot, 100);
    ui.input("say:![The shot](shot.png)");
    ui.press("enter");
    ui.wait("the first picture", |ui| replies(ui).first().is_some_and(|height| *height >= px(100.)));
    let first = replies(&mut ui)[0];
    write_png(&shot, 300);

    ui.input("say:![The shot](shot.png)");
    ui.press("enter");

    ui.wait("a second picture", |ui| replies(ui).get(1).is_some_and(|height| *height >= px(100.)));
    let drawn = replies(&mut ui);
    assert!(drawn[1] >= px(300.), "the second reply shows the file as it is now, not as it was: {drawn:?}");
    assert_eq!(drawn[0], first, "the first reply keeps the picture it showed");
}

/// Each on a line of its own, with nothing else between them, as an agent shows a before and an after.
#[gpui_kit::test]
fn images_one_after_another_are_each_drawn_as_tall_as_they_are(cx: &mut TestAppContext) {
    let (_hub, mut ui) =
        replied(cx, "![Before](before.png)\n![After](after.png)", &[("before.png", 100), ("after.png", 100)]);

    ui.wait("both pictures, one under the other", |ui| replies(ui).first().is_some_and(|height| *height >= px(200.)));
}

/// One on a line of its own, one among words, and one that's all a list's item says. Those in the same reply that the
/// kit is left to draw aren't the transcript's to open: one in a link, which goes where its link does, and one in a
/// heading.
#[gpui_kit::test]
fn a_click_on_a_replys_image_opens_it_over_the_window(cx: &mut TestAppContext) {
    let ours = "![Alone](shot.png)\n\nIt's ![among words](shot.png) here";
    let the_kits =
        "[![a link](shot.png)](https://example.com) and [in ![a link's](shot.png) words](https://example.com)";
    let reply = format!("{ours}, with {the_kits} there\n\n- ![In a list](shot.png)\n\n# ![In a heading](shot.png)");
    let (_hub, mut ui) = replied(cx, &reply, &[("shot.png", 4)]);
    ui.wait("the reply's images, from the hub", |ui| ui.shown("linked-image/").len() >= 3);

    let images = ui.shown("linked-image/");
    assert_eq!(images.len(), 3, "only the two alone and the one among words are the transcript's: {images:?}");
    for (image, what) in images.into_iter().zip(["one on a line of its own", "one among words", "one in a list"]) {
        ui.click(image);
        assert!(ui.has_dialog() && ui.shown("opened-picture").len() == 1, "a click on {what} opens it");
        ui.press("escape");
        assert!(!ui.has_dialog(), "and Escape closes {what}");
    }
}

/// A drawing is drawn at the size it says it is, as a bitmap is at its pixels, and opens as one does.
#[gpui_kit::test]
fn a_drawing_a_reply_points_to_is_drawn_as_tall_as_it_says(cx: &mut TestAppContext) {
    let drawing = br##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="120"><rect width="60" height="120" fill="#c33"/></svg>"##;
    let (_hub, mut ui) =
        replied_with(cx, "![How it flows](flow.svg)", &[("flow.svg", MediaType::Svg, drawing.to_vec())]);

    ui.wait("the drawing", |ui| replies(ui).first().is_some_and(|height| *height >= px(120.)));
    let drawn = replies(&mut ui)[0];
    assert!(drawn < px(200.), "at its own size, not twice it: {drawn:?}");
    let image = ui.shown("linked-image/").remove(0);
    ui.click(image);
    assert!(ui.has_dialog() && ui.shown("opened-picture").len() == 1, "and a click opens it");
}

/// A Mermaid diagram in a reply is drawn as a picture, as tall as it's drawn, which opens as an image does, and the
/// reply's row grows to hold it once it's drawn. Escape closes it, before it would go from the chat to the composer.
#[gpui_kit::test]
fn a_diagram_in_a_reply_is_drawn_as_a_picture_that_opens(cx: &mut TestAppContext) {
    let reply =
        "How it flows:\n\n```mermaid\nflowchart TD\n  A[Ask] --> B[Decide]\n  B --> C[Build]\n```\n\nThat's all.";
    let (_hub, mut ui) = replied_with(cx, reply, &[]);

    ui.wait("the diagram", |ui| ui.shown("diagram/").len() == 1);
    let diagram = ui.shown("diagram/").remove(0);
    let tall = ui.bounds(diagram.clone()).size.height;
    assert!(tall > px(150.), "three boxes, one under another: {tall:?}");
    ui.wait("the row grown to hold it", |ui| replies(ui).first().is_some_and(|height| *height > tall));
    assert!(ui.shown("code-block-actions").is_empty(), "drawn, not as its code");
    ui.click(diagram);
    assert!(ui.has_dialog() && ui.shown("opened-picture").len() == 1, "and a click opens it");
    ui.press("escape");
    assert!(!ui.has_dialog(), "and Escape closes it");
}

/// A diagram that can't be drawn, such as one with a mistake in it, is the code block it was written as.
#[gpui_kit::test]
fn a_diagram_that_cant_be_drawn_is_its_code(cx: &mut TestAppContext) {
    let reply = "How it flows:\n\n```mermaid\nflowchart TD\n  A[Ask --> B\n```";
    let (_hub, mut ui) = replied_with(cx, reply, &[]);

    ui.wait("its code", |ui| ui.shown("code-block-actions").len() == 1);
    assert_eq!(ui.shown("diagram/").len(), 1, "taken as a diagram, and drawn as its code");
    assert!(ui.shown("opened-picture").is_empty());
}

/// A plan the agent asks to have approved draws its diagrams too.
#[gpui_kit::test]
fn a_diagram_in_a_plan_to_approve_is_drawn(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let planned = "1. Ask.\n\n```mermaid\nflowchart TD\n  A[Ask] --> B[Build]\n```";
    let asked =
        ApprovalRequest { tool_use_id: Some(ToolUseId::from("toolu-1")), ..approval_request("ask-1", plan(planned)) };
    let events = first_turn(user_prompt("Plan it."), [typed_call("msg-1", "toolu-1", Call::Plan), asked.into()]);
    world.chat(&workspace, SessionStatus::WaitingApproval, events);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);

    ui.wait("the plan's diagram", |ui| ui.shown("diagram/").len() == 1);
}

/// The kit still draws an image the transcript leaves to it, from the copy the hub kept.
#[gpui_kit::test]
fn an_image_thats_a_link_is_still_drawn_as_tall_as_it_is(cx: &mut TestAppContext) {
    let (_hub, mut ui) = replied(cx, "[![A link](shot.png)](https://example.com)", &[("shot.png", 100)]);

    ui.wait("the picture", |ui| replies(ui).first().is_some_and(|height| *height >= px(100.)));
    assert!(ui.shown("linked-image/").is_empty(), "and it isn't the transcript's to open");
}

/// The kit looks past a mark for what a paragraph holds, so one that's only an image in bold is an image alone.
#[gpui_kit::test]
fn an_image_alone_in_bold_is_drawn_as_tall_as_it_is_and_is_the_transcripts_to_open(cx: &mut TestAppContext) {
    let (_hub, mut ui) = replied(cx, "**![In bold](shot.png)**", &[("shot.png", 100)]);

    ui.wait("the picture", |ui| replies(ui).first().is_some_and(|height| *height >= px(100.)));
    assert_eq!(ui.shown("linked-image/").len(), 1, "and it's the transcript's to open");
}

/// A reply over 4 KB is parsed on another thread, with what draws its images, so it's only as long as that needs.
#[gpui_kit::test]
fn a_click_on_a_long_replys_image_opens_it_over_the_window(cx: &mut TestAppContext) {
    let reply = format!("{}Junctions last\n\n![Alone](shot.png)", "Timetables, sidings and points. ".repeat(140));
    assert!(reply.len() > 4 * 1024, "parsed on another thread: {} bytes", reply.len());
    let (_hub, mut ui) = replied(cx, &reply, &[("shot.png", 4)]);
    ui.wait("the reply's image, from the hub", |ui| ui.shown("linked-image/").len() == 1);

    let image = ui.shown("linked-image/").remove(0);
    ui.click(image);

    assert!(ui.has_dialog() && ui.shown("opened-picture").len() == 1, "a click on it opens it");
}

/// While a reply is written, the kit draws what it points to, since the hub keeps a copy of each image only with the
/// reply's record. Then it's parsed again in the row of its record, for the transcript to draw them, and they open.
#[gpui_kit::test]
fn a_replys_images_open_once_its_recorded_after_its_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    write_png(&ui.worktree(&workspace).join("shot.png"), 4);

    // A reply that goes on until the test has seen how it's drawn while it's written.
    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("slow-until:{done}:![The shot](shot.png) after"));
    ui.press("enter");

    ui.wait("the reply being written", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("after3")))
    });
    assert!(ui.shown("linked-image/").is_empty(), "the kit draws them while it's written");
    done.open();
    ui.wait_idle_with("the turn to finish", |text| text.contains("after3"));
    let images = ui.assistant_text().matches("](shot.png)").count();
    ui.wait("the reply's images, from the hub", |ui| ui.shown("linked-image/").len() == images);
    let first = ui.shown("linked-image/").remove(0);
    ui.click(first);
    assert!(ui.has_dialog() && ui.shown("opened-picture").len() == 1, "and a click on one opens it");
}
