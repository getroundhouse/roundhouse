//! What an edit, a write or a patch would do to its files, for where it's asked about before it's made: each file's
//! change as a diff, as the call draws it once it's made.

use std::rc::Rc;

use client_core::Given;
use client_core::calls::count;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled, div};
use look::{Typeset as _, language};

use super::{files, marks, numbered, patch};
use crate::calls::lines::Section;
use crate::find::Marks;

/// What a change asked about would do, worked out from what it was given.
pub(crate) struct ChangeAsked {
    /// What it changes, as it's said above its lines: its file, or for a patch of several, how many.
    pub(crate) path: String,
    files: Vec<FileAsked>,
}

/// What a change asked about would do to one of its files.
struct FileAsked {
    /// Its path, and where it moves to, if it does, said over its lines when the change is to several files.
    label: String,
    /// The path its language is told by.
    path: String,
    /// Its change, as a diff, which is empty for a file it deletes.
    sections: Rc<Vec<Section>>,
}

impl ChangeAsked {
    /// What `given` would change, if it's an edit, a write or a patch.
    pub(crate) fn of(given: &Given) -> Option<Self> {
        let one = |path: &str, sections| vec![FileAsked { label: path.into(), path: path.into(), sections }];
        let files = match given {
            Given::Edit(edit) => one(edit.path, Rc::new(files::edited(edit, None))),
            Given::Write(write) => one(write.path, Rc::new(files::written(write, None))),
            Given::Patch(patch) => patch::asked(patch)
                .into_iter()
                .map(|(label, path, sections)| FileAsked { label, path: path.into(), sections })
                .collect(),
            Given::Command(_) | Given::MainInput(_) => return None,
        };
        let path = match files.as_slice() {
            [file] => file.label.clone(),
            files => count(files.len(), "file", "files"),
        };
        let files =
            files.into_iter().map(|file| FileAsked { sections: marks::marked(&file.sections), ..file }).collect();
        Some(Self { path, files })
    }

    /// Whether it changes several files, which is asked about as several edits.
    pub(crate) fn several(&self) -> bool {
        self.files.len() > 1
    }

    /// A row for each of its lines, as a diff, as wide as its longest, and for a change to several files, each file's
    /// path over its lines. A file it deletes says so in place of its lines. Nothing in them is found, since what's
    /// asked isn't in the transcript.
    pub(crate) fn rows(&self, cx: &App) -> Vec<AnyElement> {
        let none = Marks::default();
        let muted = look::colors(cx).text_muted;
        // A path in the code font, since it's what you'd copy or type, and a word as a detail.
        let label = || div().px(look::Space::Medium).text_color(muted);
        let mut rows = Vec::new();
        for file in &self.files {
            if self.several() {
                rows.push(label().typeset_code(cx).child(file.label.clone()).into_any_element());
            }
            if file.sections.is_empty() {
                rows.push(label().typeset(look::Type::Detail).child("Deleted").into_any_element());
            }
            let language = language(&file.path);
            rows.extend(file.sections.iter().flat_map(|section| numbered::rows(language, section, &none, None, cx)));
        }
        rows
    }
}
