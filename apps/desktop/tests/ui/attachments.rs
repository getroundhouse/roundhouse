//! Attaching files in the composer, and sending them with a prompt.

#![cfg(test)]

use std::io::Cursor;

use client_core::FileToSend;
use gpui_kit::{ClipboardEntry, ClipboardItem, ExternalPaths, Image, ImageFormat, TestAppContext};
use protocol::attachment::MediaType;
use protocol::event::EventBody;
use transcript::TranscriptEvent;

use crate::support::{Stack, Ui};

/// A real PNG, so the app can read its size, as it does for any image it shows.
fn png() -> Vec<u8> {
    encoded(image::ImageFormat::Png)
}

fn encoded(format: image::ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 3, image::Rgba([200, 30, 30, 255])).write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

fn open_a_chat<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui
}

#[gpui_kit::test]
fn a_pasted_image_goes_with_the_prompt_and_the_agent_sees_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);

    ui.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Png, png())));
    ui.wait("the image to be attached", |ui| ui.attached() == ["Pasted image.png"]);
    assert_eq!(ui.composer_text(), "", "a pasted image isn't text");
    ui.input("attached");
    ui.press("enter");

    let len = png().len();
    let said = format!("attached: image/png {len} bytes | Pasted image.png: on disk, {len} bytes");
    ui.wait_idle_with("the agent to see the image", |text| text == said);
    assert!(ui.attached().is_empty(), "sending takes what was attached");
    let carried = ui.events().into_iter().find_map(|e| match e.body {
        EventBody::UserPrompt { attachments, .. } => attachments.into_iter().next(),
        _ => None,
    });
    let carried = carried.expect("the prompt carries the image");
    assert_eq!(stack.client().attachment_read(&ui.active_chat(), &carried.id).unwrap(), png());
}

/// As some apps copy a screenshot. The agent can't be given a TIFF, so it goes as a PNG.
#[gpui_kit::test]
fn a_pasted_tiff_goes_as_a_png(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);

    ui.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Tiff, encoded(image::ImageFormat::Tiff))));
    ui.wait("the image to be attached", |ui| ui.attached() == ["Pasted image.png"]);
    ui.input("attached");
    ui.press("enter");
    ui.wait_idle_with("the agent to see a PNG", |text| text.starts_with("attached: image/png "));
}

#[gpui_kit::test]
fn copied_files_of_any_kind_attach_by_name_and_a_folder_says_why_it_cant(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);
    let dir = tempfile::tempdir().unwrap();
    let (log, blob, folder) = (dir.path().join("build.log"), dir.path().join("blob.bin"), dir.path().join("notes"));
    std::fs::write(&log, "error at line 42\n").unwrap();
    std::fs::write(&blob, [0, 1, 2]).unwrap();
    std::fs::create_dir(&folder).unwrap();

    let copied = ExternalPaths(vec![log, blob, folder].into());
    ui.paste(ClipboardItem::from(ClipboardEntry::ExternalPaths(copied)));

    ui.wait("the refusal", |ui| ui.view().error.is_some());
    assert!(ui.view().error.clone().unwrap().contains("couldn't attach notes: a folder can't be attached"));
    assert_eq!(ui.attached(), ["build.log", "blob.bin"]);
    ui.input("attached");
    ui.press("enter");
    let said = "attached: build.log: error at line 42 | blob.bin: on disk, 3 bytes";
    ui.wait_idle_with("the agent to see the files", |text| text == said);
}

#[gpui_kit::test]
fn attachments_stay_with_their_own_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);
    let first = ui.active_chat();
    ui.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Png, png())));
    ui.wait("the image to be attached", |ui| ui.attached().len() == 1);

    ui.new_chat();
    assert!(ui.attached().is_empty(), "a new chat starts with nothing attached");
    ui.show_chat(&first);

    ui.wait("the first chat's image to come back", |ui| ui.attached() == ["Pasted image.png"]);
}

/// A file can take a while to read, and someone can switch chats meanwhile.
#[gpui_kit::test]
fn a_file_read_after_switching_chats_goes_to_the_chat_it_came_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);
    let first = ui.active_chat();
    let second = ui.new_chat();
    ui.show_chat(&first);

    let pane = ui.pane().unwrap();
    let pasted = ClipboardItem::from(Image::from_bytes(ImageFormat::Png, png()));
    pane.update(ui.cx, |p, cx| {
        assert!(p.paste(&pasted, cx));
        p.activate(second.clone(), cx);
    });
    ui.frame();

    assert!(ui.attached().is_empty(), "it isn't put in the chat showing now");
    ui.show_chat(&first);
    ui.wait("the image in the chat it was pasted in", |ui| ui.attached() == ["Pasted image.png"]);
}

#[gpui_kit::test]
fn a_prompt_that_couldnt_be_sent_comes_back_to_its_own_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);
    let first = ui.active_chat();
    let first_transcript = ui.transcript().unwrap();
    let second = ui.new_chat();
    let not_sent = |text: &str| TranscriptEvent::NotSent {
        message: "couldn't send that: the connection to the hub was lost".to_string(),
        text: text.to_string(),
        files: vec![FileToSend::checked("build.log", MediaType::Text, b"error at line 42\n".to_vec()).unwrap()],
    };

    // The chat that didn't send isn't showing.
    first_transcript.update(ui.cx, |_, cx| cx.emit(not_sent("why does this fail?")));
    ui.frame();
    assert!(ui.attached().is_empty(), "the chat showing gets nothing");
    assert_eq!(ui.view().error.as_deref(), Some("couldn't send that: the connection to the hub was lost"));
    ui.show_chat(&first);
    ui.wait("the prompt back in its chat", |ui| ui.attached() == ["build.log"]);
    assert_eq!(ui.composer_text(), "why does this fail?");

    // The chat that didn't send is showing, and something's been typed since.
    ui.show_chat(&second);
    ui.input("and this");
    let second_transcript = ui.transcript().unwrap();
    second_transcript.update(ui.cx, |_, cx| cx.emit(not_sent("look")));
    ui.wait("the prompt back", |ui| ui.composer_text() == "look\n\nand this");
    assert_eq!(ui.attached(), ["build.log"]);
}
