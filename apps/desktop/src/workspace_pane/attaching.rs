//! Files waiting in the composer to go with the next prompt. They come from a paste, a drop or the file picker, each is
//! checked as it comes, so nothing that would be refused gets as far as Send, and they're drawn above what's typed.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use client_core::{FileToSend, NotAttached};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::attachment::{Attachment as Card, AttachmentGroup, AttachmentMedia};
use gpui_kit::{
    AnyElement, AppContext as _, Axis, ClipboardEntry, ClipboardItem, Context, ElementId, Image, ImageFormat,
    IntoElement, ParentElement, PathPromptOptions, Styled,
};
use protocol::attachment::{MAX_PER_PROMPT, MediaType};
use protocol::model::{AttachmentId, SessionId};

use super::{PaneEvent, WorkspacePane};

/// What a pasted image is called, since it has no name of its own. The agent sees only the image.
const PASTED_NAME: &str = "Pasted image";

/// A file in the composer, checked and ready to send.
pub(crate) struct Attached {
    file: FileToSend,
    /// What its tile shows, for an image.
    preview: Option<Arc<Image>>,
}

impl Attached {
    /// Keeps the file's ID, so one handed back after a failed send isn't added to the session twice.
    fn new(file: FileToSend) -> Self {
        let preview = transcript::image_format(file.attachment.media_type)
            .map(|format| Arc::new(Image::from_bytes(format, file.data.clone())));
        Self { file, preview }
    }

    pub(crate) fn into_file(self) -> FileToSend {
        self.file
    }
}

/// What a chat's composer holds while another chat is showing.
#[derive(Default)]
pub(crate) struct Draft {
    pub(crate) text: String,
    pub(crate) files: Vec<Attached>,
}

impl Draft {
    pub(crate) fn has_anything(&self) -> bool {
        !self.text.trim().is_empty() || !self.files.is_empty()
    }
}

impl WorkspacePane {
    /// Takes a paste that holds images or copied files, and leaves text to the composer. Returns whether it took it.
    pub fn paste(&self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
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
            return false;
        }
        self.read_then_attach(
            move || {
                images.into_iter().map(from_clipboard).chain(paths.iter().map(|path| FileToSend::read(path))).collect()
            },
            cx,
        );
        true
    }

    /// Files dropped on the composer, or chosen in the file picker.
    pub(crate) fn attach_paths(&self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.read_then_attach(move || paths.iter().map(|path| FileToSend::read(path)).collect(), cx);
    }

    pub(crate) fn pick_files(cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let _ = this.update(cx, |t, cx| t.attach_paths(paths, cx));
        })
        .detach();
    }

    /// The names of what's attached in the chat showing, in order, for a test to see what a paste or drop did.
    #[cfg(feature = "testing")]
    pub fn attached_names(&self) -> Vec<String> {
        self.attached.iter().map(|attached| attached.file.attachment.name.clone()).collect()
    }

    pub(crate) fn remove_attached(&mut self, id: &AttachmentId, cx: &mut Context<Self>) {
        self.attached.retain(|attached| &attached.file.attachment.id != id);
        cx.notify();
    }

    /// Puts back a prompt that couldn't be sent, in its own chat, so nothing typed or attached is lost. The text goes
    /// before anything typed since.
    pub(crate) fn give_back(&mut self, chat: &SessionId, text: String, files: Vec<FileToSend>, cx: &mut Context<Self>) {
        for file in files {
            if let Err(message) = self.attach_to(Some(chat), Attached::new(file)) {
                cx.emit(PaneEvent::Error(message));
            }
        }
        if !text.is_empty() {
            if self.active.as_ref() == Some(chat) {
                // Setting the composer needs the window.
                self.with_window(cx, move |t, window, cx| {
                    let text = joined(text, &t.composer.read(cx).text().to_string());
                    t.composer.update(cx, |c, cx| c.set_value(text, window, cx));
                });
            } else {
                let draft = self.drafts.entry(chat.clone()).or_default();
                draft.text = joined(text, &draft.text);
            }
        }
        cx.notify();
    }

    /// Reads and checks files off the main thread, since a dropped file can be large, then adds what passed to the
    /// chat that was showing when they came, even if another is showing by the time they're read.
    fn read_then_attach(
        &self,
        read: impl FnOnce() -> Vec<Result<FileToSend, NotAttached>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let chat = self.active.clone();
        cx.spawn(async move |this, cx| {
            let read = cx.background_spawn(async move { read() }).await;
            let _ = this.update(cx, |t, cx| {
                for file in read {
                    let attached = file.map_err(|refused| refused.to_string()).map(Attached::new);
                    if let Err(message) = attached.and_then(|file| t.attach_to(chat.as_ref(), file)) {
                        cx.emit(PaneEvent::Error(message));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Adds a checked file to `chat`'s composer, or to its draft if another chat is showing, unless the prompt
    /// couldn't carry it with the rest.
    fn attach_to(&mut self, chat: Option<&SessionId>, file: Attached) -> Result<(), String> {
        let files = match chat {
            Some(chat) if self.active.as_ref() != Some(chat) => &mut self.drafts.entry(chat.clone()).or_default().files,
            _ => &mut self.attached,
        };
        if files.len() >= MAX_PER_PROMPT {
            let name = &file.file.attachment.name;
            return Err(format!("couldn't attach {name}: a prompt can carry at most {MAX_PER_PROMPT} files"));
        }
        files.push(file);
        Ok(())
    }

    /// A row of tiles above what's typed: an image as itself, any other file as its name and size.
    pub(crate) fn attached_tiles(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.attached.is_empty() {
            return None;
        }
        let tiles = self.attached.iter().map(|attached| {
            let attachment = &attached.file.attachment;
            let (id, element) = (attachment.id.clone(), ElementId::Name(format!("attached-{}", attachment.id).into()));
            let remove = cx.listener(move |t, _, _, cx| t.remove_attached(&id, cx));
            match &attached.preview {
                Some(preview) => Card::new()
                    .id(element)
                    .small()
                    .tooltip(attachment.name.clone())
                    .axis(Axis::Vertical)
                    .media(AttachmentMedia::new().src(preview.clone()))
                    .on_remove(remove),
                None => transcript::file_card(element, attachment).on_remove(remove),
            }
        });
        Some(AttachmentGroup::new("composer-attachments").px_3().pt_3().children(tiles).into_any_element())
    }
}

/// What was handed back, then anything typed since, so neither is lost.
fn joined(back: String, since: &str) -> String {
    if since.trim().is_empty() { back } else { format!("{back}\n\n{since}") }
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
