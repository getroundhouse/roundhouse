//! Changing files with a patch, as Codex does: several at once, each added, deleted, or changed and maybe moved. A
//! patch of one file says what it did to it, as an edit or a write does, and a patch of several says how many it
//! changed. Opened, each file's path, then its change as a diff.

use std::rc::Rc;

use client_core::calls::files::file_name;
use client_core::calls::{count, typed};
use gpui_kit::assets::IconName;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement};
use look::language;
use protocol::capability::calls::call::{FilePatch, Patch, PatchChange};
use protocol::capability::calls::output::FileOutcome;
use protocol::event::FileChange;

use super::files::{chip, edit_line, whole, write_line};
use super::line::{Line, Part};
use super::{Answer, Toggle, block, line, marks, numbered};
use crate::calls::diff;
use crate::calls::lines::Section;
use crate::find::Part as Text;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

pub(super) fn draw(patch: &Patch, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let files = patched(patch, answer);
    let line = match files.as_slice() {
        [file] => file.line(),
        _ => Line::new(IconName::Files, format!("Edit {}", count(files.len(), "file", "files")))
            .part(Part::Counts(files.iter().map(|file| diff::counts(&file.sections)).sum()))
            .part(Part::Note(names(&files).into())),
    };
    let opened = || {
        let parts = files.iter().enumerate().flat_map(|(i, file)| {
            let label = block::found_label(toggle, Text::nth(Text::FoundPath, i), &file.label(), cx).into_any_element();
            let lines = (!file.sections.is_empty()).then(|| {
                let sections = marks::marked(&file.sections);
                numbered::lines(toggle, Text::nth(Text::FoundLines, i), language(file.path), &sections, cx)
            });
            let deleted = (file.does == Does::Deletes).then(|| block::label("Deleted", cx).into_any_element());
            [Some(label), lines, deleted].into_iter().flatten()
        });
        (!files.is_empty()).then(|| block::frame(cx).children(parts).into_any_element())
    };
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding.
pub(super) fn shown(patch: &Patch, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    let files = patched(patch, answer);
    line::shown_failure(answer, false, key, gather);
    gather.inside(key, Fold::Output, |gather| {
        for (i, file) in files.iter().enumerate() {
            gather.plain(key, Text::nth(Text::FoundPath, i), &file.label());
            if !file.sections.is_empty() {
                gather.plain(key, Text::nth(Text::FoundLines, i), &numbered::shown_text(&file.sections));
            }
        }
    });
}

/// One file of a patch, as it's drawn.
struct Patched<'a> {
    path: &'a str,
    /// Where it went, when it moved too.
    moved_to: Option<&'a str>,
    does: Does,
    /// What it did to the file, as a diff: from what came back, with the file's own line numbers, or, before then or
    /// where its agent didn't say, from the patch itself, without them.
    sections: Rc<Vec<Section>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Does {
    /// Writes a file of this many lines, which `replaced` one that was there, when what came back says so.
    Adds {
        lines: usize,
        replaced: bool,
    },
    Deletes,
    /// Changes what's in it, or only moves it, or something this build doesn't know.
    Changes,
}

impl Patched<'_> {
    /// The line of a patch of this file alone, which says what it did to the file as an edit or a write does.
    fn line(&self) -> Line {
        match self.does {
            Does::Adds { lines, replaced } => {
                write_line(self.path, lines, replaced.then(|| diff::counts(&self.sections)))
            }
            Does::Deletes => Line::new(IconName::Trash, "Delete").part(chip(self.path)),
            Does::Changes => {
                let title = if self.moved_to.is_some() { "Move" } else { "Edit" };
                edit_line(title, self.path, self.moved_to.unwrap_or(self.path), &self.sections)
            }
        }
    }

    /// Its path, and where it went, when it moved.
    fn label(&self) -> String {
        self.moved_to.map_or_else(|| self.path.to_string(), |to| format!("{} → {to}", self.path))
    }
}

/// What `patch` would do to each of its files, worked out from the patch, for where it's asked about before it's made:
/// each file's path, where it moves to, if it does, and its change as a diff, which is empty for one it deletes.
pub(super) fn asked(patch: &Patch) -> Vec<(String, &str, Rc<Vec<Section>>)> {
    patched(patch, None).into_iter().map(|file| (file.label(), file.path, file.sections)).collect()
}

/// The names of the files, which is what a line has room for.
fn names(files: &[Patched]) -> String {
    files.iter().map(|file| file_name(file.moved_to.unwrap_or(file.path))).collect::<Vec<_>>().join(", ")
}

/// Each file of `patch`, with what came back for it, if anything has.
fn patched<'a>(patch: &'a Patch, answer: Option<&'a Answer>) -> Vec<Patched<'a>> {
    let output = answer.map(|answer| &*answer.output);
    patch.files.iter().map(|file| one(file, typed::outcome(file, output))).collect()
}

fn one<'a>(file: &'a FilePatch, outcome: Option<&FileOutcome>) -> Patched<'a> {
    let returned = outcome.and_then(|outcome| match &outcome.change {
        Some(FileChange::Changed { hunks }) => Some(diff::from_hunks(hunks)),
        Some(FileChange::Created | FileChange::Unknown) | None => None,
    });
    let (does, moved_to, sections) = match &file.change {
        PatchChange::Add { content } => {
            let does = Does::Adds { lines: content.lines().count(), replaced: returned.is_some() };
            (does, None, returned.unwrap_or_else(|| vec![whole(content)]))
        }
        PatchChange::Delete => (Does::Deletes, None, Vec::new()),
        PatchChange::Update { move_to, hunks } => {
            let from_patch = || hunks.iter().map(|hunk| diff::unnumbered(&hunk.lines)).collect();
            (Does::Changes, move_to.as_deref(), returned.unwrap_or_else(from_patch))
        }
        PatchChange::Unknown => (Does::Changes, None, returned.unwrap_or_default()),
    };
    Patched { path: &file.path, moved_to, does, sections: Rc::new(sections) }
}
