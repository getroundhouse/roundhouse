//! Searching the workspace: the pattern, where, and how much it found. Opened, matching lines grouped by file, or the
//! files it found.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, div};

use super::line::{Line, Part};
use super::{Answer, Toggle, block, line, numbered};
use crate::calls::files::{file_name, language};
use crate::calls::lines::NumberedLine;
use crate::calls::{Found, Search};

/// More files than are worth listing in a block.
const LISTED_FILES: usize = 200;

pub(super) fn draw(search: &Search, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let found = answer.and_then(Answer::success).map(Found::read);
    let line = Line::new(IconName::Search, search.title())
        .part(Part::Pill(search.pattern.to_string().into()))
        .part(search.path.map(|path| Part::Chip(file_name(path).to_string().into())))
        .part(found.as_ref().map(|found| Part::Note(found.summary().into())));
    let opened = found.filter(|_| toggle.open).and_then(|found| opened(&found, toggle, cx));
    line::drawn(line, answer, opened, toggle, cx)
}

fn opened(found: &Found, toggle: &Toggle, cx: &App) -> Option<AnyElement> {
    let id = toggle.key.element("found");
    let shown = match found {
        Found::Lines(files) => {
            let mut rows = Vec::new();
            for file in files {
                rows.push(block::label(file.path.to_string(), cx).into_any_element());
                let lines: Vec<NumberedLine> =
                    file.lines.iter().map(|(number, text)| NumberedLine::same(*number, text)).collect();
                rows.extend(numbered::rows(language(file.path), &lines, cx));
            }
            numbered::scrolling(id.clone(), div().id(id).pb_2().children(rows)).into_any_element()
        }
        Found::Files(files) => {
            let listed = files.iter().take(LISTED_FILES).copied().collect::<Vec<_>>().join("\n");
            block::output(id, &listed, cx.theme().foreground, cx)
        }
        Found::Raw(text) => block::output(id, text, cx.theme().foreground, cx),
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
