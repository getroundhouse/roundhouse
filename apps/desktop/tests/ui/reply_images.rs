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
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
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
