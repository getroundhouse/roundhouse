//! Searching the workspace: the pattern, where, and how much it found. Opened, matching lines grouped by file, or the
//! files it found.

use client_core::calls::files::file_name;
use client_core::calls::{FileLines, Found, Search};
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, div};
use look::language;

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line, numbered};
use crate::calls::lines::NumberedLine;
use crate::find::Part as Text;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

/// More files than are worth listing in a block.
const LISTED_FILES: usize = 200;

pub(super) fn draw(search: &Search, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let found = answer.and_then(Answer::success).map(Found::read);
    let line = Line::new(IconName::Search, search.title())
        .part(Part::Pill(search.pattern.to_string().into(), toggle.marks(Text::Pill, search.pattern)))
        .part(search.path.map(|path| Part::Chip(file_name(path).to_string().into())))
        .part(found.as_ref().map(|found| Part::Note(found.summary().into())));
    let opened = || found.and_then(|found| opened(&found, toggle, cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding.
pub(super) fn shown(search: &Search, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    gather.plain(key, Text::Pill, search.pattern);
    line::shown_failure(answer, false, key, gather);
    let Some(found) = answer.and_then(Answer::success).map(Found::read) else { return };
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
        Found::Nothing => {}
    });
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
            numbered::scrolling(toggle, &id, div().id(id.clone()).pb_2().children(rows)).into_any_element()
        }
        Found::Files(_) | Found::Raw(_) => {
            block::output(toggle, Text::Found, &listed(found)?, look::colors(cx).text, cx)
        }
        Found::Nothing => return None,
    };
    Some(
        block::frame(cx)
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .child(shown)
            .into_any_element(),
    )
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
        Found::Lines(_) | Found::Nothing => None,
    }
}
