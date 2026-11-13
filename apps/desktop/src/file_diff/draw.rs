//! How each row of a file's diff is drawn: a line with its numbers, how it changed, and its code, a fold with the
//! arrows that open it, and a comment on the band it sits on. Lines added and removed are tinted green and red, as
//! review tools tint them, and the words that changed within them more strongly.

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::TextareaState;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Entity, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled, TestSupportExt as _, WeakEntity,
    div, px,
};
use look::{Hand as _, Shaped as _, Typeset as _};
use protocol::model::{DiffComment, DiffSide};

use super::FileDiff;
use super::annotations::{comment_card, draft_card};
use super::draft::LineAt;
use super::layout::{Fold, Row, STEP};
use super::selection::{DrawnLines, Selection};
use super::sides::Sides;
use crate::file_view::LINE_HEIGHT;
use crate::patch::LineKind;
use code::{Code, code};

mod code;

/// Taller than a line, so a fold reads as a band across the code rather than as a line of it.
pub(super) const FOLD_HEIGHT: Pixels = px(28.);
/// The column between the line numbers and the code, which says how a line changed.
pub(super) const SIGN: Pixels = px(16.);
/// Each line's group, whose hover shows its button to add a comment.
const LINE: &str = "diff-line";

/// What every row of a frame is drawn from.
pub(super) struct Drawing {
    pub(super) sides: Arc<Sides>,
    pub(super) path: SharedString,
    /// How wide each column of line numbers is.
    pub(super) numbers: Pixels,
    /// How wide the line numbers before a fold's text are: both sides', in one column, or the old side's.
    pub(super) gutter: Pixels,
    /// The line the comment being written is on, which stays marked.
    pub(super) marked: Option<(DiffSide, u32)>,
    pub(super) comments: Vec<DiffComment>,
    /// The comment being written, and whether it's a saved one being edited.
    pub(super) draft: Option<(Entity<TextareaState>, bool)>,
    /// How far the code is scrolled sideways.
    pub(super) across: Pixels,
    pub(super) selection: Option<Selection>,
    /// Where each line's text is drawn, noted as it's drawn.
    pub(super) drawn: DrawnLines,
    pub(super) view: WeakEntity<FileDiff>,
}

/// Row `ix`, which is `row`.
pub(super) fn row(ix: usize, row: &Row, drawing: &Drawing, cx: &App) -> AnyElement {
    match row {
        Row::Line { kind, old, new } => line(ix, *kind, *old, *new, drawing, cx),
        Row::Pair { old, new, same } => h_flex()
            .w_full()
            .h(LINE_HEIGHT)
            .typeset(look::Type::Code)
            .child(half(ix, true, *old, *new, *same, drawing, cx))
            .child(half(ix, false, *new, *new, *same, drawing, cx).border_l_1().border_color(look::colors(cx).edge))
            .into_any_element(),
        Row::Fold(fold) => fold_row(fold, drawing, cx),
        Row::Comment(id) => match drawing.comments.iter().find(|comment| &comment.id == id) {
            Some(comment) => comment_card(comment.clone(), drawing.view.clone(), cx),
            None => div().into_any_element(),
        },
        Row::Draft => match &drawing.draft {
            Some((input, editing)) => draft_card(input, *editing, drawing.view.clone(), cx),
            None => div().into_any_element(),
        },
    }
}

/// A line in one column, with both its numbers.
fn line(ix: usize, kind: LineKind, old: Option<u32>, new: Option<u32>, drawing: &Drawing, cx: &App) -> AnyElement {
    let sides = &drawing.sides;
    // A line both sides have is drawn as the new side has it.
    let code = match (kind, old, new) {
        (LineKind::Removed, Some(old), _) => {
            Code { row: ix, old: true, line: sides.old.line(old), changed: sides.old.changed(old) }
        }
        (_, _, Some(new)) => Code { row: ix, old: false, line: sides.new.line(new), changed: sides.new.changed(new) },
        _ => Code { row: ix, old: false, line: None, changed: &[] },
    };
    // A comment on a line both sides have is on the new one, which is the one the agent will read.
    let at = match kind {
        LineKind::Removed => old.map(|old| (DiffSide::Old, old)),
        LineKind::Same | LineKind::Added => new.map(|new| (DiffSide::New, new)),
    };
    h_flex()
        .group(LINE)
        .w_full()
        .h(LINE_HEIGHT)
        .typeset(look::Type::Code)
        .when_some(background(kind, at, drawing, cx), Styled::bg)
        .child(number(old, drawing.numbers, cx))
        .child(number(new, drawing.numbers, cx))
        .child(changes(kind, &code, drawing, at, cx))
        .into_any_element()
}

/// One side of a row of two: the old side's line, or the new side's, or nothing where the side has none there.
/// `paired` is the new side's line of a row both sides have, which a comment on either goes on.
fn half(
    ix: usize,
    old_side: bool,
    line: Option<u32>,
    paired: Option<u32>,
    same: bool,
    drawing: &Drawing,
    cx: &App,
) -> Div {
    let half = div().w_1_2().h_full().min_w_0();
    let Some(number_) = line else { return half.bg(look::colors(cx).inset.opacity(0.3)) };
    let side = if old_side { &drawing.sides.old } else { &drawing.sides.new };
    let kind = match (same, old_side) {
        (true, _) => LineKind::Same,
        (false, true) => LineKind::Removed,
        (false, false) => LineKind::Added,
    };
    let at = match kind {
        LineKind::Same => paired.map(|new| (DiffSide::New, new)),
        LineKind::Removed => Some((DiffSide::Old, number_)),
        LineKind::Added => Some((DiffSide::New, number_)),
    };
    let changed = if same { &[][..] } else { side.changed(number_) };
    let code = Code { row: ix, old: old_side, line: side.line(number_), changed };
    half.child(
        h_flex()
            .group(LINE)
            .size_full()
            .when_some(background(kind, at, drawing, cx), Styled::bg)
            .child(number(line, drawing.numbers, cx))
            .child(changes(kind, &code, drawing, at, cx)),
    )
}

/// A line's tint: green for one added, red for one removed, and the selection's colour for the one being commented on.
fn background(kind: LineKind, at: Option<(DiffSide, u32)>, drawing: &Drawing, cx: &App) -> Option<Hsla> {
    let colors = look::colors(cx);
    if at.is_some() && at == drawing.marked {
        return Some(colors.selection());
    }
    match kind {
        LineKind::Same => None,
        LineKind::Removed => Some(colors.removed.opacity(0.12)),
        LineKind::Added => Some(colors.added.opacity(0.12)),
    }
}

/// What a line is drawn as after its numbers: how it changed, its code, and its button to add a comment, over the
/// start of its code.
fn changes(kind: LineKind, code: &Code<'_>, drawing: &Drawing, at: Option<(DiffSide, u32)>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let (sign, tint) = match kind {
        LineKind::Same => (" ", None),
        LineKind::Removed => ("−", Some(colors.removed)),
        LineKind::Added => ("+", Some(colors.added)),
    };
    let marked = at.is_some() && at == drawing.marked;
    // Placed from the start of what it's in, which is where the sign is.
    h_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(div().flex_none().w(SIGN).text_color(tint.unwrap_or(colors.text_muted)).child(sign))
        .child(self::code(code, tint.map(|tint| tint.opacity(0.3)), drawing, cx))
        .children(at.map(|at| add_button(at, marked, drawing, cx)))
        .into_any_element()
}

fn number(line: Option<u32>, width: Pixels, cx: &App) -> Div {
    let text = line.map(|line| SharedString::from(line.to_string()));
    div().flex_none().w(width).pr_2().text_right().text_color(look::colors(cx).text_faint).children(text)
}

/// The button that starts a comment on a line, which shows while the line is hovered, and while it's `marked` as the
/// one being commented on.
fn add_button((side, line): (DiffSide, u32), marked: bool, drawing: &Drawing, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let (view, at) = (drawing.view.clone(), LineAt { path: drawing.path.clone(), side, line });
    let name = match side {
        DiffSide::Old => "old",
        DiffSide::New | DiffSide::Unknown => "new",
    };
    div()
        .id(ElementId::Name(format!("add-comment-{name}-{line}").into()))
        .test_support()
        .absolute()
        .top_0()
        .left_0()
        .size(LINE_HEIGHT)
        .flex()
        .items_center()
        .justify_center()
        .corners(look::Corner::Control)
        .bg(colors.main)
        .text_color(colors.on_main)
        .hand()
        .when(!marked, |button| button.opacity(0.).group_hover(LINE, |button| button.opacity(1.)))
        .child(Icon::new(IconName::Plus).xsmall())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |t, cx| t.start_comment(at.clone(), window, cx));
        })
        .into_any_element()
}

/// Lines git left out, with how many there are, and the arrows in the line numbers' column that open them: up to
/// show the lines just above the change below, down to show those just below the change above, each [`STEP`] lines
/// at a time, or all of them at once when there are no more than that between two changes.
fn fold_row(fold: &Fold, drawing: &Drawing, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let lines = fold.old.end - fold.old.start;
    let (start, end) = (fold.old.start, fold.old.end);
    let mut arrows = Vec::new();
    if fold.opens && !fold.top && !fold.end && lines <= STEP || fold.opens && fold.top && fold.end {
        arrows.push(("fold-all", IconName::ChevronsUpDown, format!("Show the {lines} lines"), start..end));
    } else if fold.opens {
        if !fold.top {
            let shown = lines.min(STEP);
            arrows.push((
                "fold-down",
                IconName::ChevronDown,
                format!("Show {shown} lines below"),
                start..start + shown,
            ));
        }
        if !fold.end {
            let shown = lines.min(STEP);
            arrows.push(("fold-up", IconName::ChevronUp, format!("Show {shown} lines above"), end - shown..end));
        }
    }
    let arrows = arrows.into_iter().map(|(id, icon, tooltip, opens)| {
        let view = drawing.view.clone();
        Button::new((id, start)).hand().ghost().xsmall().icon(Icon::new(icon)).tooltip(tooltip).on_click(
            move |_, _, cx| {
                let _ = view.update(cx, |t, cx| t.open(opens.clone(), cx));
            },
        )
    });
    let said = format!("{lines} unchanged line{}", if lines == 1 { "" } else { "s" });
    h_flex()
        .w_full()
        .h(FOLD_HEIGHT)
        .bg(colors.inset)
        .text_color(colors.text_muted)
        .child(h_flex().flex_none().w(drawing.gutter).justify_center().gap_1().children(arrows))
        .child(div().pl(SIGN).typeset(look::Type::Detail).child(said))
        .into_any_element()
}
