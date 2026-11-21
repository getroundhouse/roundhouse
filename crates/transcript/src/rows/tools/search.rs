//! Searching the workspace, with a search of its own or a command that only searches: the pattern, where, and how much
//! it found. Opened, matching lines grouped by file, or the files it found.

use client_core::calls::files::file_name;
use client_core::calls::{FileLines, Found, Search};
use gpui_kit::assets::IconName;
use gpui_kit::{AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, div};
use look::{Typeset as _, language};

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line, numbered};
use crate::calls::lines::NumberedLine;
use crate::find::Part as Text;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

/// What a search looked in, by its path. The call doesn't say whether that's a folder or a file, and it's a folder more
/// often, so it's taken for a file only where its name has an extension.
fn looked_in(path: &str) -> look::ChipKind {
    if file_name(path).contains('.') { look::ChipKind::File } else { look::ChipKind::Folder }
}

/// More files than are worth listing in a block.
const LISTED_FILES: usize = 200;

pub(super) fn draw(search: &Search, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let found = found(answer);
    let line = Line::new(IconName::Search, search.title())
        .part(Part::Pill(search.pattern.to_string().into(), toggle.marks(Text::Pill, search.pattern)))
        .part(
            search.path.map(|path| Part::Chip(look::chip(looked_in(path), look::ChipSize::Regular, path.to_string()))),
        )
        .part(found.as_ref().map(|found| Part::Note(found.summary().into())));
    let opened = || found.and_then(|found| opened(&found, toggle, cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding.
pub(super) fn shown(search: &Search, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    gather.plain(key, Text::Pill, search.pattern);
    line::shown_failure(answer, false, key, gather);
    let Some(found) = found(answer) else { return };
    gather.inside(key, Fold::Output, |gather| match &found {
        Found::Lines(files) => {
            for (i, file) in files.iter().enumerate() {
                gather.plain(key, Text::nth(Text::FoundPath, i), file.path);
                gather.plain(key, Text::nth(Text::FoundLines, i), &numbered::rows_text(&lines(file)));
            }
        }
        Found::Files(_) | Found::Raw(_) => {
            if let Some(text) = listed(&found) {
                gather.plain(key, Text::Found, &block::output_text(&text));
            }
        }
        Found::Count { .. } | Found::Nothing => {}
    });
}

/// What it found, once it ran.
fn found(answer: Option<&Answer>) -> Option<Found<'_>> {
    let answer = answer?;
    Some(Found::of(answer.success()?, &answer.output))
}

fn opened(found: &Found, toggle: &Toggle, cx: &App) -> Option<AnyElement> {
    let id = Text::Found.element(toggle.key);
    let shown = match found {
        Found::Lines(files) => {
            let area = block::scroll_id(&id);
            let mut rows = Vec::new();
            for (i, file) in files.iter().enumerate() {
                rows.push(block::found_label(toggle, Text::nth(Text::FoundPath, i), file.path, cx).into_any_element());
                let lines = lines(file);
                let marks = toggle.find.marks_of(&(toggle.key.clone(), Text::nth(Text::FoundLines, i)), || {
                    numbered::rows_text(&lines).into()
                });
                rows.extend(numbered::rows(language(file.path), &lines, &marks, Some(&area), cx));
            }
            numbered::scrolling(toggle, &id, div().id(id.clone()).pb(look::Space::Small).children(rows))
                .into_any_element()
        }
        Found::Files(_) | Found::Raw(_) => {
            block::output(toggle, Text::Found, &listed(found)?, look::colors(cx).text, cx)
        }
        Found::Count { .. } | Found::Nothing => return None,
    };
    Some(block::frame(cx).typeset_code(cx).child(shown).into_any_element())
}

/// A file's matching lines, with their numbers.
fn lines(file: &FileLines) -> Vec<NumberedLine> {
    file.lines.iter().map(|(number, text)| NumberedLine::same(*number, text)).collect()
}

/// What a search that found files, or something else, shows: the files, one to a line, or what came.
fn listed(found: &Found) -> Option<String> {
    match found {
        Found::Files(files) => Some(files.iter().take(LISTED_FILES).copied().collect::<Vec<_>>().join("\n")),
        Found::Raw(text) => Some((*text).to_string()),
        Found::Lines(_) | Found::Count { .. } | Found::Nothing => None,
    }
}
