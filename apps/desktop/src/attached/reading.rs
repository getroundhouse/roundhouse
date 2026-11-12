//! Reading and checking files as they come, from a paste, a drop or the file picker, off the main thread, since a
//! dropped file can be large.

use std::io::Cursor;
use std::path::PathBuf;

use client_core::{FileToSend, NotAttached};
use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, Context, Image, ImageFormat, PathPromptOptions};
use protocol::attachment::MediaType;

use super::{Attached, Attachments};

/// What a pasted image is called, since it has no name of its own. The agent sees only the image.
const PASTED_NAME: &str = "Pasted image";

/// Reads files and checks them, off the main thread.
pub(crate) type Reading = Box<dyn FnOnce() -> Vec<Result<FileToSend, NotAttached>> + Send>;

/// What to read for a paste that holds images or copied files, or `None` for one that's only text, which is left to
/// the field it's pasted in.
pub(crate) fn pasted(item: &ClipboardItem) -> Option<Reading> {
    let mut images = Vec::new();
    let mut paths = Vec::new();
    for entry in item.entries() {
        match entry {
            ClipboardEntry::Image(image) => images.push(image.clone()),
            ClipboardEntry::ExternalPaths(copied) => paths.extend_from_slice(copied.paths()),
            ClipboardEntry::String(_) => {}
        }
    }
    if images.is_empty() && paths.is_empty() {
        return None;
    }
    Some(Box::new(move || {
        images.into_iter().map(from_clipboard).chain(paths.iter().map(|path| FileToSend::read(path))).collect()
    }))
}

/// Files dropped, or chosen in the file picker.
pub(crate) fn from_paths(paths: Vec<PathBuf>) -> Reading {
    Box::new(move || paths.iter().map(|path| FileToSend::read(path)).collect())
}

/// Asks for files to attach, and hands what was chosen to `then`.
pub(crate) fn pick<T: 'static>(
    cx: &mut Context<T>,
    then: impl FnOnce(&mut T, Vec<PathBuf>, &mut Context<T>) + 'static,
) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("Attach".into()),
    });
    cx.spawn(async move |this, cx| {
        let Ok(Ok(Some(paths))) = picked.await else { return };
        let _ = this.update(cx, |t, cx| then(t, paths, cx));
    })
    .detach();
}

/// Reads off the main thread, then adds each file that passed to the list `files` finds in `T`, then hands `added`
/// what to say about each one refused, or that the list had no room for.
pub(crate) fn read_into<T: 'static>(
    reading: Reading,
    cx: &mut Context<T>,
    files: impl FnOnce(&mut T) -> &mut Attachments + 'static,
    added: impl FnOnce(&mut T, Vec<String>, &mut Context<T>) + 'static,
) {
    cx.spawn(async move |this, cx| {
        let read = cx.background_spawn(async move { reading() }).await;
        let _ = this.update(cx, |t, cx| {
            let mut problems = Vec::new();
            let list = files(t);
            for file in read {
                match file {
                    Ok(file) => problems.extend(list.push(Attached::new(file)).err()),
                    Err(not_attached) => problems.push(not_attached.to_string()),
                }
            }
            added(t, problems, cx);
            cx.notify();
        });
    })
    .detach();
}

/// A pasted image. A TIFF, which some apps copy, or a BMP becomes a PNG, so the agent can be given it in the prompt and
/// the transcript can draw it. Any other kind goes as the file it is, for the agent to open.
fn from_clipboard(image: Image) -> Result<FileToSend, NotAttached> {
    let name = |extension: &str| format!("{PASTED_NAME}.{extension}");
    if matches!(image.format, ImageFormat::Tiff | ImageFormat::Bmp) {
        let unreadable = || NotAttached { name: name("png"), why: "the pasted image couldn't be read".to_string() };
        let png = to_png(&image.bytes).ok_or_else(unreadable)?;
        return FileToSend::checked(&name("png"), MediaType::Png, png);
    }
    FileToSend::checked(&name(image.format.extension()), MediaType::sniff(&image.bytes), image.bytes)
}

fn to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let mut png = Cursor::new(Vec::new());
    decoded.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}
