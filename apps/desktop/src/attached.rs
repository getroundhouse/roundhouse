//! Files waiting to go with a prompt that hasn't been sent, in a chat's composer or the new-workspace dialog: the list
//! of them, drawn as tiles above what's typed, and the file picker and drop target that add to it. Each file is read
//! and checked as it comes, in `reading`, so nothing that would be refused gets as far as sending. A prompt that can't
//! be sent comes back with its files.

use std::path::PathBuf;
use std::sync::Arc;

use client_core::FileToSend;
use gpui_kit::component::attachment::{Attachment as Card, AttachmentGroup, AttachmentMedia};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::{Axis, Context, ElementId, ExternalPaths, Image, InteractiveElement, ParentElement, Styled};
use protocol::attachment::MAX_PER_PROMPT;
use protocol::model::AttachmentId;

mod reading;

pub(crate) use reading::{pasted, pick, read_into};

/// A file waiting to go, checked and ready to send.
pub(crate) struct Attached {
    file: FileToSend,
    /// What its tile shows, for an image.
    preview: Option<Arc<Image>>,
}

impl Attached {
    /// Keeps the file's ID, so one handed back after a failed send isn't added to the session twice.
    pub(crate) fn new(file: FileToSend) -> Self {
        let preview = transcript::image_format(file.attachment.media_type)
            .map(|format| Arc::new(Image::from_bytes(format, file.data.clone())));
        Self { file, preview }
    }

    fn name(&self) -> &str {
        &self.file.attachment.name
    }
}

/// The files waiting to go with one prompt, in the order they came, never more than a prompt can carry.
#[derive(Default)]
pub(crate) struct Attachments(Vec<Attached>);

impl Attachments {
    /// Adds a file, unless a prompt couldn't carry it with the rest, and then says why.
    pub(crate) fn push(&mut self, file: Attached) -> Result<(), String> {
        if self.0.len() >= MAX_PER_PROMPT {
            let name = file.name();
            return Err(format!("couldn't attach {name}: a prompt can carry at most {MAX_PER_PROMPT} files"));
        }
        self.0.push(file);
        Ok(())
    }

    /// Puts back the files of a prompt that couldn't be sent, then any added since, and says why for each that
    /// didn't fit.
    pub(crate) fn put_back(&mut self, files: Vec<FileToSend>) -> Vec<String> {
        let since = std::mem::take(&mut self.0);
        let back = files.into_iter().map(Attached::new).chain(since);
        back.filter_map(|file| self.push(file).err()).collect()
    }

    /// Adds another list's files after these, as [`Attachments::put_back`] does.
    pub(crate) fn append(&mut self, other: Attachments) -> Vec<String> {
        other.0.into_iter().filter_map(|file| self.push(file).err()).collect()
    }

    pub(crate) fn remove(&mut self, id: &AttachmentId) {
        self.0.retain(|file| &file.file.attachment.id != id);
    }

    /// Takes every file, to send.
    pub(crate) fn take(&mut self) -> Vec<FileToSend> {
        std::mem::take(&mut self.0).into_iter().map(|attached| attached.file).collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Their names, in order, for a test to see what a paste or drop did.
    #[cfg(feature = "testing")]
    pub(crate) fn names(&self) -> Vec<String> {
        self.0.iter().map(|file| file.name().to_string()).collect()
    }

    /// A row of tiles above what's typed: an image as itself, any other file as its name and size. Each tile's remove
    /// button calls `remove` with its file's ID. The caller pads it, to line up with what's typed under it.
    pub(crate) fn tiles<T: 'static>(
        &self,
        group: &'static str,
        cx: &mut Context<T>,
        remove: impl Fn(&mut T, &AttachmentId, &mut Context<T>) + Clone + 'static,
    ) -> Option<AttachmentGroup> {
        if self.0.is_empty() {
            return None;
        }
        let tiles = self.0.iter().map(|attached| {
            let attachment = &attached.file.attachment;
            let (id, element) = (attachment.id.clone(), ElementId::Name(format!("attached-{}", attachment.id).into()));
            let remove = remove.clone();
            let remove = cx.listener(move |t, _, _, cx| remove(t, &id, cx));
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
        Some(AttachmentGroup::new(group).children(tiles))
    }
}

/// Whether there's a prompt to send: something typed, or a file.
pub(crate) fn anything_to_send(text: &str, files: usize) -> bool {
    !text.trim().is_empty() || files > 0
}

/// Takes files dropped anywhere on `target`, which shows the focus colour on its edge while they're dragged over it.
pub(crate) fn takes_drops<T: 'static, E: InteractiveElement>(
    target: E,
    cx: &mut Context<T>,
    then: impl Fn(&mut T, Vec<PathBuf>, &mut Context<T>) + 'static,
) -> E {
    let ring = cx.theme().ring;
    target
        .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(ring))
        .on_drop(cx.listener(move |t, dropped: &ExternalPaths, _, cx| then(t, dropped.paths().to_vec(), cx)))
}

/// Reads files dropped or chosen in the file picker into the list `files` finds in `T`, as [`read_into`] does.
pub(crate) fn read_paths_into<T: 'static>(
    paths: Vec<PathBuf>,
    cx: &mut Context<T>,
    files: impl FnOnce(&mut T) -> &mut Attachments + 'static,
    refused: impl Fn(&mut T, String, &mut Context<T>) + 'static,
) {
    read_into(reading::from_paths(paths), cx, files, refused);
}

/// A prompt handed back because it couldn't be sent, then anything typed since, so neither is lost.
pub(crate) fn joined(back: String, since: &str) -> String {
    if since.trim().is_empty() { back } else { format!("{back}\n\n{since}") }
}
