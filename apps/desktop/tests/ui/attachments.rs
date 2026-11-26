//! Attaching files in the composer, sending them with a prompt, and opening an image a prompt carried.

#![cfg(test)]

use std::io::Cursor;

use client_core::FileToSend;
use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, first_turn, text};
use gpui_kit::component::TITLE_BAR_HEIGHT;
use gpui_kit::{
    AppContext as _, Bounds, ClipboardEntry, ClipboardItem, ElementId, ExternalPaths, Image, ImageFormat, SharedString,
    TestAppContext, point, px, size,
};
use protocol::attachment::{Attachment, MediaType};
use protocol::event::EventBody;
use protocol::model::{AttachmentId, SessionStatus};
use protocol::prompt::Prompt;
use transcript::TranscriptEvent;

use crate::support::{FakeHub, Stack, Ui};
use protocol::prompt::PromptFrom;

/// A real PNG, so the app can read its size, as it does for any image it shows.
pub(crate) fn png() -> Vec<u8> {
    encoded(image::ImageFormat::Png)
}

fn encoded(format: image::ImageFormat) -> Vec<u8> {
    sized(4, 3, format)
}

fn sized(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255])).write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

fn open_a_chat<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui
}

/// A TIFF, as some apps copy a screenshot, goes as a PNG, since the agent can't be given a TIFF.
#[gpui_kit::test]
fn a_pasted_image_goes_with_the_prompt_and_the_agent_sees_it_and_a_tiff_goes_as_a_png(cx: &mut TestAppContext) {
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
        EventBody::UserPrompt { prompt: Prompt { files: attachments, .. }, .. } => attachments.into_iter().next(),
        _ => None,
    });
    let carried = carried.expect("the prompt carries the image");
    assert_eq!(stack.client().attachment_read(&ui.active_chat(), &carried.id).unwrap(), png());

    ui.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Tiff, encoded(image::ImageFormat::Tiff))));
    ui.wait("the TIFF to be attached", |ui| ui.attached() == ["Pasted image.png"]);
    ui.input("attached");
    ui.press("enter");
    // The first turn's words start the same, so it's the second turn's that are waited for, after them.
    ui.wait_idle_with("the agent to see the TIFF as a PNG", |text| {
        text.strip_prefix(said.as_str()).is_some_and(|second| second.starts_with("attached: image/png "))
    });
}

#[gpui_kit::test]
fn files_chosen_from_the_add_menu_are_attached(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);
    let dir = tempfile::tempdir().unwrap();
    let (notes, shot) = (dir.path().join("notes.txt"), dir.path().join("shot.png"));
    std::fs::write(&notes, "the plan\n").unwrap();
    std::fs::write(&shot, png()).unwrap();

    ui.click(SharedString::from("add"));
    ui.click_menu_item("Attach files…");
    ui.wait("the file picker", |ui| ui.cx.did_prompt_for_paths());
    ui.cx.simulate_path_prompt_response(|asked| {
        assert!(asked.files && asked.multiple && !asked.directories, "it asks for any number of files");
        Some(vec![notes.clone(), shot.clone()])
    });
    ui.wait("the files to be attached", |ui| ui.attached() == ["notes.txt", "shot.png"]);
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
    assert!(ui.view().error.clone().unwrap().contains("Couldn't attach notes: a folder can't be attached"));
    assert_eq!(ui.attached(), ["build.log", "blob.bin"]);
    ui.input("attached");
    ui.press("enter");
    let said = "attached: build.log: error at line 42 | blob.bin: on disk, 3 bytes";
    ui.wait_idle_with("the agent to see the files", |text| text == said);
}

/// Even when the file comes once someone has switched chats, since a file can take a while to read.
#[gpui_kit::test]
fn attachments_stay_with_their_own_chat_even_read_after_switching_chats(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);
    let first = ui.active_chat();
    ui.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Png, png())));
    ui.wait("the image to be attached", |ui| ui.attached().len() == 1);

    let second = ui.new_chat();
    assert!(ui.attached().is_empty(), "a new chat starts with nothing attached");
    ui.show_chat(&first);
    ui.wait("the first chat's image to come back", |ui| ui.attached() == ["Pasted image.png"]);

    let pane = ui.pane().unwrap();
    let pasted = ClipboardItem::from(Image::from_bytes(ImageFormat::Png, png()));
    pane.update(ui.cx, |p, cx| {
        assert!(p.paste(&pasted, cx));
        p.activate(second.clone(), cx);
    });
    ui.frame();
    assert!(ui.attached().is_empty(), "the image read after switching isn't put in the chat showing now");
    ui.show_chat(&first);
    ui.wait("the image read after switching, in the chat it was pasted in", |ui| ui.attached().len() == 2);
    assert_eq!(ui.attached(), ["Pasted image.png", "Pasted image.png"]);
}

#[gpui_kit::test]
fn a_prompt_that_couldnt_be_sent_comes_back_to_its_own_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_a_chat(cx, &stack);
    let first = ui.active_chat();
    let first_transcript = ui.transcript().unwrap();
    let second = ui.new_chat();
    let not_sent = |text: &str| TranscriptEvent::NotSent {
        message: "Couldn't send that: the connection to the hub was lost".to_string(),
        prompt: Prompt::carrying(
            text,
            vec![FileToSend::checked("build.log", MediaType::Text, b"error at line 42\n".to_vec()).unwrap()],
        ),
    };

    // The chat that didn't send isn't showing.
    first_transcript.update(ui.cx, |_, cx| cx.emit(not_sent("why does this fail?")));
    ui.frame();
    assert!(ui.attached().is_empty(), "the chat showing gets nothing");
    assert_eq!(ui.view().error.as_deref(), Some("Couldn't send that: the connection to the hub was lost"));
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

/// Over the window rather than in the chat, so nothing in the chat moves, and it shows even when its prompt is at the
/// bottom of the window, as one jumped to from the outline is. What a prompt carried is the hub's, so this opens on a
/// fake hub holding a chat whose prompt carried an image, with no worker or agent to wait for.
#[gpui_kit::test]
fn a_prompts_image_opens_over_the_window_as_big_as_fits_and_a_click_or_escape_closes_it(cx: &mut TestAppContext) {
    // Wider than any window, so it has to shrink to fit.
    let shot = sized(4000, 100, image::ImageFormat::Png);
    let size_bytes = u64::try_from(shot.len()).unwrap();
    let carried = Attachment {
        id: AttachmentId::from("shot-1"),
        name: "shot.png".into(),
        media_type: MediaType::Png,
        size_bytes,
    };
    let mut world = World::default();
    world.attachments.insert(carried.id.clone(), shot);
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let prompt = EventBody::UserPrompt {
        client_msg_id: None,
        prompt: Prompt::carrying("Look.".to_string(), vec![carried.clone()]),
        from: PromptFrom::Person,
    };
    world.chat(&workspace, SessionStatus::Idle, first_turn(prompt, [text("seen", "Seen."), finished_turn(3, 0.01)]));
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    let thumbnail = ElementId::Name(format!("picture/attachment/{}", carried.id).into());
    // Drawn from the hub's copy, as wide as a thumbnail goes, since the image is forty times as wide as it's tall, and
    // until then as wide as a thumbnail's guessed to be, which is under twice its height.
    ui.wait("the image's thumbnail", |ui| {
        ui.transcript().is_some() && ui.has(thumbnail.clone()) && {
            let drawn = ui.bounds(thumbnail.clone());
            drawn.size.width > drawn.size.height * 2.
        }
    });
    let before = ui.bounds(thumbnail.clone());

    ui.click(thumbnail.clone());

    assert!(ui.has_dialog(), "it opens over the window");
    let (opened, dialog) = (ui.bounds("opened-picture"), ui.bounds("surface"));
    let window = ui.cx.update_window(ui.handle, |_, window, _| window.viewport_size()).unwrap();
    let clear = TITLE_BAR_HEIGHT * 2.;
    let room =
        Bounds::new(point(TITLE_BAR_HEIGHT, TITLE_BAR_HEIGHT), size(window.width - clear, window.height - clear));
    assert!(room.contains(&opened.origin) && room.contains(&opened.bottom_right()), "{opened:?} fits in {room:?}");
    // To the pixel, which layout rounds to.
    assert!((opened.size.height - opened.size.width / 40.).abs() < px(1.), "{opened:?} keeps its shape");
    assert_eq!(dialog, opened.dilate(px(1.)), "the dialog is the image and its edge");
    let off_middle = opened.center() - point(window.width / 2., window.height / 2.);
    assert!(off_middle.x.abs() < px(1.) && off_middle.y.abs() < px(1.), "{opened:?} is in the middle of {window:?}");
    assert_eq!(ui.bounds(thumbnail.clone()), before, "nothing in the chat moves");
    ui.press("escape");
    assert!(!ui.has_dialog(), "Escape closes it");

    ui.click(thumbnail.clone());
    ui.click("opened-picture");
    assert!(!ui.has_dialog(), "and so does a click on it");

    ui.click(thumbnail);
    ui.click_at(point(opened.center().x, opened.bottom() + px(20.)));
    assert!(!ui.has_dialog(), "or around it");
}
