//! Files waiting to go with a prompt that hasn't been sent, in a chat's composer or the new-workspace dialog: the list
//! of them, drawn as tiles above what's typed, and the file picker and drop target that add to it. Each file is read
//! and checked as it comes, in `reading`, so nothing that would be refused gets as far as sending. A prompt that can't
//! be sent comes back with its files. The hub keeps a chat's with its draft, and the dialog's with what's in it, so each
//! says where it was given to.

use std::path::PathBuf;
use std::sync::Arc;

use client_core::{FileToSend, FilesFor};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::attachment::{Attachment as Card, AttachmentGroup, AttachmentMedia};
use gpui_kit::{Axis, Context, ElementId, ExternalPaths, Image, InteractiveElement, ParentElement, Styled};
use protocol::attachment::MAX_PER_PROMPT;
use protocol::model::AttachmentId;
use protocol::prompt::PromptProblem;

mod from_the_hub;
mod reading;

pub(crate) use from_the_hub::read_back;
pub(crate) use reading::{pasted, pick, read_into};

/// A file waiting to go, checked and ready to send.
pub(crate) struct Attached {
    file: FileToSend,
    /// What its tile shows, for an image.
    preview: Option<Arc<Image>>,
    /// Which draft the hub was given it for, a chat's or the new-workspace dialog's, to keep with what's typed there.
    kept_for: Option<FilesFor>,
}

impl Attached {
    /// Keeps the file's ID, so one handed back after a failed send isn't added to the session twice.
    pub(crate) fn new(file: FileToSend) -> Self {
        let preview = transcript::image_format(file.attachment.media_type)
            .map(|format| Arc::new(Image::from_bytes(format, file.data.clone())));
        Self { file, preview, kept_for: None }
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
            return Err(format!("couldn't attach {name}: {}", PromptProblem::TooManyFiles));
        }
        self.0.push(file);
        Ok(())
    }

    /// Puts back the files of a prompt that couldn't be sent, then any added since, and says why for each that
    /// didn't fit.
    pub(crate) fn put_back(&mut self, files: Vec<FileToSend>) -> Vec<String> {
        self.put_first(files.into_iter().map(Attached::new).collect())
    }

    /// Puts back the files the hub kept for a draft, as [`Attachments::put_back`] does.
    pub(crate) fn bring_back(&mut self, files: Vec<FileToSend>, kept_for: &FilesFor) -> Vec<String> {
        let kept = |file| Attached { kept_for: Some(kept_for.clone()), ..Attached::new(file) };
        self.put_first(files.into_iter().map(kept).collect())
    }

    fn put_first(&mut self, files: Vec<Attached>) -> Vec<String> {
        let since = std::mem::take(&mut self.0);
        files.into_iter().chain(since).filter_map(|file| self.push(file).err()).collect()
    }

    /// Adds another list's files after these, as [`Attachments::put_back`] does.
    pub(crate) fn append(&mut self, other: Attachments) -> Vec<String> {
        other.0.into_iter().filter_map(|file| self.push(file).err()).collect()
    }

    /// Takes a file out, and says which draft the hub was given it for, if any.
    pub(crate) fn remove(&mut self, id: &AttachmentId) -> Option<FilesFor> {
        let at = self.0.iter().position(|file| &file.file.attachment.id == id)?;
        self.0.remove(at).kept_for
    }

    /// Keeps only the files `keep` picks, in their order, such as those a draft made elsewhere still names.
    pub(crate) fn retain(&mut self, keep: impl Fn(&AttachmentId) -> bool) {
        self.0.retain(|file| keep(&file.file.attachment.id));
    }

    /// The files the hub hasn't been given for a draft, to give it, and which from now on it's taken to have.
    pub(crate) fn not_yet_given(&mut self, kept_for: &FilesFor) -> Vec<FileToSend> {
        let new = self.0.iter_mut().filter(|file| file.kept_for.as_ref() != Some(kept_for));
        new.map(|file| {
            file.kept_for = Some(kept_for.clone());
            file.file.clone()
        })
        .collect()
    }

    /// The IDs of the files the hub was given for a draft.
    pub(crate) fn given_for<'a>(&'a self, kept_for: &'a FilesFor) -> impl Iterator<Item = AttachmentId> + 'a {
        let given = self.0.iter().filter(move |file| file.kept_for.as_ref() == Some(kept_for));
        given.map(|file| file.file.attachment.id.clone())
    }

    /// Each file's ID, in order, for the draft that names them.
    pub(crate) fn ids(&self) -> impl Iterator<Item = AttachmentId> + '_ {
        self.0.iter().map(|file| file.file.attachment.id.clone())
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

/// Takes files dropped anywhere on `target`, which shows the focus colour on its edge while they're dragged over it.
pub(crate) fn takes_drops<T: 'static, E: InteractiveElement>(
    target: E,
    cx: &mut Context<T>,
    then: impl Fn(&mut T, Vec<PathBuf>, &mut Context<T>) + 'static,
) -> E {
    let ring = look::colors(cx).brand;
    target
        .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(ring))
        .on_drop(cx.listener(move |t, dropped: &ExternalPaths, _, cx| then(t, dropped.paths().to_vec(), cx)))
}

/// Reads files dropped or chosen in the file picker into the list `files` finds in `T`, as [`read_into`] does.
pub(crate) fn read_paths_into<T: 'static>(
    paths: Vec<PathBuf>,
    cx: &mut Context<T>,
    files: impl FnOnce(&mut T) -> &mut Attachments + 'static,
    added: impl FnOnce(&mut T, Vec<String>, &mut Context<T>) + 'static,
) {
    read_into(reading::from_paths(paths), cx, files, added);
}

/// A prompt handed back because it couldn't be sent, then anything typed since, so neither is lost.
pub(crate) fn joined(back: String, since: &str) -> String {
    if since.trim().is_empty() { back } else { format!("{back}\n\n{since}") }
}
