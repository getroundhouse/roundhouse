//! Changing one cell of a notebook: replacing what it holds, adding a cell after it, or deleting it. Opened, the
//! notebook's path, then what the cell holds afterwards, all of it added, since the call doesn't say what it held
//! before.

use std::rc::Rc;

use client_core::calls::typed;
use gpui_kit::assets::IconName;
use gpui_kit::{AnyElement, App};
use protocol::capability::calls::call::{CellEdit, NotebookEdit};

use super::files::{chip, shown_file, shown_lines, whole};
use super::line::Line;
use super::{Answer, Toggle, line, marks};
use crate::calls::lines::Section;
use crate::item_key::ItemKey;
use crate::rows::shown::Gather;

pub(super) fn draw(edit: &NotebookEdit, answer: Option<&Answer>, toggle: &Toggle, cx: &App) -> AnyElement {
    let line = Line::new(IconName::NotebookPen, typed::notebook_title(edit)).part(chip(&edit.path));
    let sections = Rc::new(source(edit));
    let opened = || (!sections.is_empty()).then(|| shown_file(toggle, &edit.path, &marks::marked(&sections), cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding.
pub(super) fn shown(edit: &NotebookEdit, answer: Option<&Answer>, key: &ItemKey, gather: &mut Gather) {
    let sections = source(edit);
    if sections.is_empty() {
        line::shown_failure(answer, false, key, gather);
    } else {
        shown_lines(&edit.path, &sections, answer, key, gather);
    }
}

/// What the cell holds afterwards, numbered from its top, or nothing, for a cell deleted.
fn source(edit: &NotebookEdit) -> Vec<Section> {
    match edit.does {
        CellEdit::Delete => Vec::new(),
        CellEdit::Replace | CellEdit::InsertAfter | CellEdit::Unknown if edit.source.is_empty() => Vec::new(),
        CellEdit::Replace | CellEdit::InsertAfter | CellEdit::Unknown => vec![whole(&edit.source)],
    }
}
