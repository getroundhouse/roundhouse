//! Images in the workspace's file tab: one opened from All files, drawn with what kind it is and how big, and one the
//! workspace changes, as it was and as it is, side by side, one side at a time, or as what changed. The worker reads
//! each side, and how the window shows them is the app's, so those open on a fake hub holding made-up images, with no
//! worker to wait for. Only reading an image again once it's changed needs the real one.

#![cfg(test)]

use std::io::Cursor;
use std::path::PathBuf;

use client_core::fake_hub::World;
use desktop::image_diff::{Comparison, Shown, comparison};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::attachment::{AttachmentBytes, MediaType, describe_size};
use protocol::model::{CommitId, FileContents, GitDiff, SessionStatus, WorktreeFiles};

use crate::support::{Diff, FakeHub, Stack, Ui, git};

/// A PNG `width` by `height` pixels, all one colour.
fn png(width: u32, height: u32) -> Vec<u8> {
    marked_png(width, height, &[])
}

/// A PNG `width` by `height` pixels, all one colour but for each of `marks`, which is another.
fn marked_png(width: u32, height: u32, marks: &[(u32, u32)]) -> Vec<u8> {
    let mut image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    for (x, y) in marks {
        image.put_pixel(*x, *y, image::Rgba([30, 30, 200, 255]));
    }
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// Where the image after differs from the one before, inside what both cover: six pixels, close together.
const MARKS: [(u32, u32); 6] = [(5, 5), (6, 5), (7, 5), (5, 6), (6, 6), (7, 6)];

/// What the file tab says of a PNG.
fn described(png: &[u8], width: u32, height: u32) -> String {
    format!("PNG · {width} × {height} · {}", describe_size(png.len() as u64))
}

/// What the file tab shows of each side of the changed file it shows, before and after.
fn sides(ui: &Ui<'_>, diff: &Diff) -> (String, String) {
    let image = ui.cx.read_entity(&diff.file, |f, _| f.image.clone());
    ui.cx.read_entity(&image, |image, _| image.described())
}

/// What the strip under the changed file's bar says of what's shown of it, part by part.
fn said(ui: &mut Ui<'_>, diff: &Diff) -> Vec<String> {
    let image = ui.cx.read_entity(&diff.file, |f, _| f.image.clone());
    ui.cx.update(|cx| image.read(cx).said_in_strip(cx))
}

/// How big a PNG is, in pixels and bytes, as the strip says when there's no need to say what kind it is.
fn sized(png: &[u8], width: u32, height: u32) -> String {
    format!("{width} × {height} · {}", describe_size(png.len() as u64))
}

/// Opens `path` from Changes, and waits for both its sides to be read.
fn compare(ui: &mut Ui<'_>, diff: &Diff, path: &str) -> (String, String) {
    ui.open_diff_of(diff, path);
    ui.wait("both sides", |ui| {
        let (before, after) = sides(ui, diff);
        before != "loading" && after != "loading"
    });
    sides(ui, diff)
}

/// What git says of the change [`changing_an_image`] makes, which only says each image differs.
const PATCH: &str = "diff --git a/gone.png b/gone.png
deleted file mode 100644
index 1a2b3c4..0000000
Binary files a/gone.png and /dev/null differ
diff --git a/new.png b/new.png
new file mode 100644
index 0000000..5d6e7f8
Binary files /dev/null and b/new.png differ
diff --git a/shot.png b/shot.png
index 9a8b7c6..5e4d3c2 100644
Binary files a/shot.png and b/shot.png differ
";

/// An image, as the worker reads one.
fn read_as_image(data: &[u8]) -> FileContents {
    FileContents::Image { media_type: MediaType::Png, data: AttachmentBytes::new(data.to_vec()) }
}

/// The window on a fake hub holding `world`, once it's put one workspace in it, showing, with its chat, with nothing in
/// it. The hub goes with the test, so it's returned to be kept.
fn in_a_workspace(
    cx: &mut TestAppContext,
    world: impl FnOnce(&mut World, &protocol::model::WorkspaceId),
) -> (FakeHub, Ui<'_>) {
    let mut made = World::default();
    let repo = made.repo("signals");
    let workspace = made.workspace(&repo, "ledger");
    made.chat(&workspace, SessionStatus::Idle, Vec::new());
    world(&mut made, &workspace);
    let fake = FakeHub::holding(made);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the workspace", |ui| ui.view().selected() == Some(&workspace) && ui.pane().is_some());
    (fake, ui)
}

/// The window on a fake hub holding a workspace whose base has `shot.png`, 30 by 20, which the workspace makes 40 by
/// 20 with [`MARKS`] in it, and `gone.png`, which it removes, and which adds `new.png`, with its sidebar of changes
/// open, and the images before, after and added.
fn changing_an_image(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, Diff, [Vec<u8>; 3]) {
    let (before, after, added) = (png(30, 20), marked_png(40, 20, &MARKS), png(10, 10));
    let (fake, mut ui) = in_a_workspace(cx, |world, workspace| {
        let base_commit = Some(CommitId::from("3f4c19c8a2d1e0b7f6a5c4d3e2f1a0b9c8d7e6f5"));
        world.diffs.insert(workspace.clone(), GitDiff { base_commit, patch: PATCH.to_string(), truncated: false });
        world.base_files.insert("shot.png".to_string(), read_as_image(&before));
        world.base_files.insert("gone.png".to_string(), read_as_image(&png(20, 10)));
        world.worktree_files.insert("shot.png".to_string(), read_as_image(&after));
        world.worktree_files.insert("new.png".to_string(), read_as_image(&added));
    });
    let diff = ui.open_changes();
    (fake, ui, diff, [before, after, added])
}

/// A workspace whose repository has `shot.png`, 30 by 20, which the workspace makes 40 by 20 with [`MARKS`] in it, with
/// its worktree, and the image before.
fn changing_an_image_on_disk<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, Diff, PathBuf, Vec<u8>) {
    let (before, after) = (png(30, 20), marked_png(40, 20, &MARKS));
    std::fs::write(stack.repo().join("shot.png"), &before).unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "shot"]);
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    let worktree = ui.worktree(&workspace);
    std::fs::write(worktree.join("shot.png"), &after).unwrap();
    let diff = ui.open_changes();
    (ui, diff, worktree, before)
}

/// What it is goes by its bytes, which is how the worker reads it, as its own tests pin, so a screenshot saved without
/// `.png` is still drawn.
#[gpui_kit::test]
fn an_image_opened_from_all_files_says_what_kind_it_is_and_how_big(cx: &mut TestAppContext) {
    let shot = png(30, 20);
    let (_hub, mut ui) = in_a_workspace(cx, |world, _| {
        world.worktree = WorktreeFiles { paths: vec!["shot".to_string()], folders: Vec::new(), truncated: false };
        world.worktree_files.insert("shot".to_string(), read_as_image(&shot));
    });
    ui.click("sidebar-tab-all-files");
    ui.wait("All files to list it", |ui| ui.has("all-file-shot"));
    let viewer = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.viewer.clone())).unwrap();

    ui.click("all-file-shot");

    ui.wait("the image", |ui| ui.cx.read_entity(&viewer, |v, _| v.shown_image().is_some()));
    assert_eq!(ui.cx.read_entity(&viewer, |v, _| v.shown_image()), Some(described(&shot, 30, 20)));
}

/// Before is as the diff's base has it, and after as the worktree does.
#[gpui_kit::test]
fn a_changed_image_shows_as_it_was_and_as_it_is_side_by_side_to_start_with(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, [before, after, _]) = changing_an_image(cx);

    let shown = compare(&mut ui, &diff, "shot.png");

    assert_eq!(shown, (described(&before, 30, 20), described(&after, 40, 20)));
    assert!(ui.has("images-side-by-side"));
    assert_eq!(ui.cx.update(|cx| comparison(cx)), Comparison::SideBySide);
    let each = [format!("Before {}", described(&before, 30, 20)), format!("After {}", described(&after, 40, 20))];
    assert_eq!(said(&mut ui, &diff), each, "each over its column");
}

/// Before and After in its bar, a click on it, and Space each flip it, and the strip under the bar names the side shown.
#[gpui_kit::test]
fn a_changed_image_flips_between_its_sides_in_one_place(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, [before, after, _]) = changing_an_image(cx);
    compare(&mut ui, &diff, "shot.png");
    let image = ui.cx.read_entity(&diff.file, |f, _| f.image.clone());
    let shown = |ui: &Ui<'_>| ui.cx.read_entity(&image, |image, _| image.shown());

    ui.click("image-before");

    assert!(ui.has("images-flip") && !ui.has("images-side-by-side"));
    assert_eq!(shown(&ui), Shown::Before);
    assert_eq!(said(&mut ui, &diff), [format!("Before {}", described(&before, 30, 20))]);
    ui.press("space");
    assert_eq!(shown(&ui), Shown::After, "Space flips it");
    assert_eq!(said(&mut ui, &diff), [format!("After {}", described(&after, 40, 20))]);
    ui.click("images-flip");
    assert_eq!(shown(&ui), Shown::Before, "and so does a click on it");
    ui.click("image-after");
    assert_eq!(shown(&ui), Shown::After);
    assert_eq!(ui.cx.update(|cx| comparison(cx)), Comparison::Flip, "kept for every workspace");
}

/// From their top left, so only the thirty by twenty both have is compared, and the ten columns only the image after
/// has aren't counted, and the strip says how big each is.
#[gpui_kit::test]
fn a_changed_image_shows_what_changed_pixel_for_pixel(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, [before, after, _]) = changing_an_image(cx);
    compare(&mut ui, &diff, "shot.png");
    let image = ui.cx.read_entity(&diff.file, |f, _| f.image.clone());
    assert_eq!(ui.cx.read_entity(&image, |image, _| image.difference()), None, "not compared until it's chosen");

    ui.click("image-difference");

    ui.wait("them to be compared", |ui| ui.cx.read_entity(&image, |image, _| image.difference()).is_some());
    let difference = ui.cx.read_entity(&image, |image, _| image.difference());
    assert_eq!(difference.as_deref(), Some("6 pixels in 1 place"));
    assert!(ui.has("images-difference"));
    let both = [format!("Before {}", sized(&before, 30, 20)), format!("After {}", sized(&after, 40, 20))];
    let told = [both[0].clone(), both[1].clone(), "6 pixels in 1 place".to_string()];
    assert_eq!(said(&mut ui, &diff), told, "both, without what kind, since they're the same");
}

#[gpui_kit::test]
fn an_added_image_has_only_the_side_after(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, [_, _, added]) = changing_an_image(cx);

    let shown = compare(&mut ui, &diff, "new.png");

    assert_eq!(shown, ("none".to_string(), described(&added, 10, 10)));
    assert!(!ui.has("image-after") && !ui.has("image-difference"), "nothing to compare it with");
    assert_eq!(said(&mut ui, &diff), [format!("Added {}", described(&added, 10, 10))]);
}

#[gpui_kit::test]
fn a_removed_image_has_only_the_side_before(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, _) = changing_an_image(cx);

    let shown = compare(&mut ui, &diff, "gone.png");

    let gone = png(20, 10);
    assert_eq!(shown, (described(&gone, 20, 10), "none".to_string()));
    assert_eq!(said(&mut ui, &diff), [format!("Removed {}", described(&gone, 20, 10))]);
}

/// So a file opened from Changes shows as it is now first, as every other comparison does.
#[gpui_kit::test]
fn a_file_opened_while_before_is_chosen_starts_at_after(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, _) = changing_an_image(cx);
    compare(&mut ui, &diff, "shot.png");
    ui.click("image-before");
    let image = ui.cx.read_entity(&diff.file, |f, _| f.image.clone());
    assert_eq!(ui.cx.read_entity(&image, |image, _| image.shown()), Shown::Before);

    compare(&mut ui, &diff, "new.png");
    compare(&mut ui, &diff, "shot.png");

    assert_eq!(ui.cx.read_entity(&image, |image, _| image.shown()), Shown::After);
    assert_eq!(ui.cx.update(|cx| comparison(cx)), Comparison::Flip, "still flipped between them");
}

/// Each window keeps a copy on the GPU of each image it draws, which stays until it's let go of. Cmd-W closes the tab
/// while the window is being updated, which GPUI leaves out of letting go of an image then.
#[gpui_kit::test]
fn a_changed_image_lets_go_of_its_pictures_once_its_tab_closes(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, _) = changing_an_image(cx);
    compare(&mut ui, &diff, "shot.png");
    ui.click("image-difference");
    let image = ui.cx.read_entity(&diff.file, |f, _| f.image.clone());
    ui.wait("them to be compared", |ui| ui.cx.read_entity(&image, |image, _| image.difference()).is_some());
    ui.frame();
    let pictures = ui.cx.read_entity(&image, |image, _| image.pictures());
    let on_gpu = |ui: &mut Ui<'_>| {
        let handle = ui.handle;
        let pictures = pictures.clone();
        ui.cx
            .update_window(handle, move |_, window, _| {
                pictures.iter().filter(|p| window.has_image_atlas_entry(p)).count()
            })
            .unwrap()
    };
    assert_eq!(on_gpu(&mut ui), 3, "before, after and what changed, as they've been drawn");

    ui.press("secondary-w");

    ui.wait("them to be let go of", |ui| on_gpu(ui) == 0);
}

#[gpui_kit::test]
fn the_image_after_is_read_again_with_the_diff(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff, worktree, before) = changing_an_image_on_disk(cx, &stack);
    compare(&mut ui, &diff, "shot.png");
    let again = png(50, 20);
    std::fs::write(worktree.join("shot.png"), &again).unwrap();

    ui.open_changes();

    ui.wait("the new image", |ui| sides(ui, &diff).1 == described(&again, 50, 20));
    assert_eq!(sides(&ui, &diff).0, described(&before, 30, 20), "the base hasn't changed");
}
