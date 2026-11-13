//! Images a reply points to in the worktree.

#![cfg(test)]

use std::io::Cursor;
use std::path::Path;

use gpui_kit::{AppContext as _, Pixels, TestAppContext, px};
use protocol::event::{ContentBlock, EventBody};

use crate::support::{Stack, Ui};

/// Writes a PNG `height` pixels tall at `path`, so a reply that shows it is at least that tall.
fn write_png(path: &Path, height: u32) {
    let mut png = Cursor::new(Vec::new());
    let image = image::RgbaImage::from_pixel(4, height, image::Rgba([200, 30, 30, 255]));
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    std::fs::write(path, png.into_inner()).unwrap();
}

/// How tall each reply that points to an image is drawn, in order.
fn replies(ui: &mut Ui<'_>) -> Vec<Pixels> {
    ui.frame();
    let transcript = ui.transcript().unwrap();
    ui.cx.read_entity(&transcript, |t, _| {
        let points = |row: &usize| {
            t.event_at(*row).is_some_and(|event| match event.body {
                EventBody::Assistant { blocks, .. } => {
                    blocks.iter().any(|block| matches!(block, ContentBlock::Text { text, .. } if text.contains("![")))
                }
                _ => false,
            })
        };
        (0..t.row_count()).filter(points).filter_map(|row| t.list().row_height(row)).collect()
    })
}

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
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    for name in ["before.png", "after.png"] {
        write_png(&ui.worktree(&workspace).join(name), 100);
    }

    ui.input("say:![Before](before.png)\n![After](after.png)");
    ui.press("enter");

    ui.wait("both pictures, one under the other", |ui| replies(ui).first().is_some_and(|height| *height >= px(200.)));
}

/// One on a line of its own, one among words, and one that's all a list's item says. Those in the same reply that the
/// kit is left to draw aren't the transcript's to open: one in a link, which goes where its link does, and one in a
/// heading.
#[gpui_kit::test]
fn a_click_on_a_replys_image_opens_it_over_the_window(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    write_png(&ui.worktree(&workspace).join("shot.png"), 4);
    let ours = "![Alone](shot.png)\n\nIt's ![among words](shot.png) here";
    let the_kits =
        "[![a link](shot.png)](https://example.com) and [in ![a link's](shot.png) words](https://example.com)";

    ui.input(&format!("say:{ours}, with {the_kits} there\n\n- ![In a list](shot.png)\n\n# ![In a heading](shot.png)"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("In a heading"));
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

/// The kit still draws an image the transcript leaves to it, from the copy the hub kept.
#[gpui_kit::test]
fn an_image_thats_a_link_is_still_drawn_as_tall_as_it_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    write_png(&ui.worktree(&workspace).join("shot.png"), 100);

    ui.input("say:[![A link](shot.png)](https://example.com)");
    ui.press("enter");

    ui.wait("the picture", |ui| replies(ui).first().is_some_and(|height| *height >= px(100.)));
    assert!(ui.shown("linked-image/").is_empty(), "and it isn't the transcript's to open");
}

/// The kit looks past a mark for what a paragraph holds, so one that's only an image in bold is an image alone.
#[gpui_kit::test]
fn an_image_alone_in_bold_is_drawn_as_tall_as_it_is_and_is_the_transcripts_to_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    write_png(&ui.worktree(&workspace).join("shot.png"), 100);

    ui.input("say:**![In bold](shot.png)**");
    ui.press("enter");

    ui.wait("the picture", |ui| replies(ui).first().is_some_and(|height| *height >= px(100.)));
    assert_eq!(ui.shown("linked-image/").len(), 1, "and it's the transcript's to open");
}

/// A reply over 4 KB is parsed on another thread, with what draws its images.
#[gpui_kit::test]
fn a_click_on_a_long_replys_image_opens_it_over_the_window(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    write_png(&ui.worktree(&workspace).join("shot.png"), 4);
    let reply = format!("{}Junctions last\n\n![Alone](shot.png)", "Timetables, sidings and points. ".repeat(140));
    assert!(reply.len() > 4 * 1024, "parsed on another thread: {} bytes", reply.len());

    ui.input(&format!("say:{reply}"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("Junctions last"));
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

    ui.input("slow:20:![The shot](shot.png) after");
    ui.press("enter");

    ui.wait("the reply being written", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("after3")))
    });
    assert!(ui.shown("linked-image/").is_empty(), "the kit draws them while it's written");
    ui.wait_idle_with("the turn to finish", |text| text.contains("after19"));
    ui.wait("the reply's images, from the hub", |ui| ui.shown("linked-image/").len() == 20);
    let first = ui.shown("linked-image/").remove(0);
    ui.click(first);
    assert!(ui.has_dialog() && ui.shown("opened-picture").len() == 1, "and a click on one opens it");
}
