//! The outline's list: every prompt in the chat, numbered, up to two lines each, and every compaction among them, over
//! the ticks.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Icon;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ListAlignment, ListOffset, ListSizingBehavior,
    ListState, ParentElement, Pixels, Role, SharedString, StatefulInteractiveElement, Styled, TestSupportExt as _,
    Window, div, list, px,
};
use look::{Hand as _, LineText};
use protocol::model::{CompactionLine, PromptLine};

use crate::rows::{RULE_ICON, compaction};

use super::{Jumps, Kind, Line};

#[cfg(test)]
mod tests;

/// Room for two lines of most prompts, and narrow enough to leave the chat beside the list.
const WIDTH: Pixels = px(360.);
/// Around the rows, inside the edge.
const INSET: Pixels = px(4.);
const EDGE: Pixels = px(1.);
/// The list's width less [`INSET`] and [`EDGE`] on each side. It's set rather than left to the list, because the list
/// measures a row before it knows how wide it is, which would count a long prompt's lines unwrapped.
const ROW_WIDTH: Pixels = px(350.);
/// The tallest the rows get before they scroll: about eight prompts of two lines.
const TALLEST: Pixels = px(420.);
const GAP: Pixels = px(2.);
/// Room for a prompt's number, up to four digits.
const NUMBER_WIDTH: Pixels = px(28.);
/// How far past the list's window rows are laid out, so one is ready before it scrolls in.
const OVERDRAW: Pixels = px(200.);
/// How many prompts show above the one being read when the list opens, so it's seen among those before it.
const LEAD: usize = 2;

/// The list's rows, kept for as long as the chat is, so each is measured once rather than each time the list opens.
/// They follow the chat's prompts whether or not the list is open.
pub(super) struct Rows {
    state: ListState,
    lines: Rc<[Line]>,
}

impl Default for Rows {
    fn default() -> Self {
        Self { state: ListState::new(0, ListAlignment::Top, OVERDRAW).measure_all(), lines: Rc::from([]) }
    }
}

impl Rows {
    /// Makes the rows those of `lines`, measuring again only the ones that changed.
    pub(super) fn follow(&mut self, lines: &Rc<[Line]>) {
        let (changed, count) = changed(&self.lines, lines);
        if changed.is_empty() && count == 0 {
            return;
        }
        // A row put in is measured once it's in view, which is enough: the list is as tall as the rows in view.
        self.state.splice(changed, count);
        self.lines = lines.clone();
    }

    /// Scrolls so the prompt at `being_read` shows, with [`LEAD`] before it, as the list opens. It goes by place rather
    /// than by how tall each row is, since none may have been measured yet. A list that can't go that far stops at its
    /// end.
    pub(super) fn open_at(&self, being_read: usize) {
        self.state.scroll_to(ListOffset { item_ix: being_read.saturating_sub(LEAD), offset_in_item: px(0.) });
    }
}

/// What changed from `was` to `now`, as the rows of `was` to replace and how many rows of `now` replace them: those
/// between the start and the end the two share. A prompt recorded joins the end, one the hub lists can come anywhere,
/// and one whose text changed stays where it was.
fn changed(was: &[Line], now: &[Line]) -> (Range<usize>, usize) {
    let start = was.iter().zip(now).take_while(|(was, now)| was == now).count();
    let end = was[start..].iter().rev().zip(now[start..].iter().rev()).take_while(|(was, now)| was == now).count();
    (start..was.len() - end, now.len() - start - end)
}

/// The list, with the prompt at `being_read` lit, as a hovered row is. It's as tall as its rows, up to [`TALLEST`],
/// and only the rows in view are drawn. The wheel over it scrolls only the list.
pub(super) fn draw(
    lines: Rc<[Line]>,
    being_read: usize,
    rows: &Rows,
    jumps: Jumps,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
    line_text: LineText,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let count = lines.len();
    let rows = list(rows.state.clone(), move |ix, _, cx| {
        div()
            .when(ix + 1 < count, |d| d.pb(GAP))
            .child(match &lines[ix].kind {
                Kind::Prompt { line, number } => {
                    row(lines[ix].row, line, *number, ix == being_read, &jumps, &line_text, cx)
                }
                Kind::Compaction(compaction) => compaction_row(lines[ix].row, compaction, &jumps, &line_text, cx),
            })
            .into_any_element()
    })
    .with_sizing_behavior(ListSizingBehavior::Infer)
    .w(ROW_WIDTH)
    // The list measures as tall as all its rows, so it's the card that stops at the tallest, with the list shrinking
    // to fit inside it, rather than the card growing to the list's whole height.
    .flex_1()
    .min_h_0();
    v_flex()
        .id("outline-list")
        .role(Role::ListBox)
        .aria_label("Jump to a prompt or a compaction")
        .w(WIDTH)
        .max_h(TALLEST + (INSET + EDGE) * 2.)
        .p(INSET)
        // The wheel scrolls the list and not the chat under it.
        .occlude()
        .rounded(px(10.))
        .border(EDGE)
        .border_color(theme.border)
        .bg(theme.popover)
        .shadow_lg()
        .on_hover(on_hover)
        .child(rows)
        .test_support()
        .into_any_element()
}

/// The prompt drawn in the chat's `row`: its number, smaller, on the baseline of its first line, and up to two lines
/// of it, quieter than the one being read.
fn row(
    row: usize,
    prompt: &PromptLine,
    number: usize,
    lit: bool,
    jumps: &Jumps,
    line_text: &LineText,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let fill = theme.secondary_hover;
    let label = label(prompt);
    let number = div().w(NUMBER_WIDTH).flex_none().text_right().text_color(look::faint()).child(number.to_string());
    let number = line_text.small.set(number);
    let text = div()
        .flex_1()
        .min_w_0()
        .line_clamp(2)
        .text_ellipsis()
        .text_color(if lit { theme.foreground } else { theme.muted_foreground })
        .child(label.clone());
    let text = line_text.body.set(text);
    div()
        .id(ElementId::Name(format!("outline-prompt/{row}").into()))
        .role(Role::ListBoxOption)
        .aria_label(label)
        .aria_selected(lit)
        .w(ROW_WIDTH)
        .flex()
        .gap_3()
        .pl_1()
        .pr_3()
        .py_2()
        .rounded(px(8.))
        .hand()
        .when(lit, |d| d.bg(fill))
        .hover(move |d| d.bg(fill))
        .on_click(jumps.to(row))
        .child(number)
        .child(text)
        .test_support()
        .into_any_element()
}

/// The compaction drawn in the chat's `row`: its icon where a prompt's number goes, then what its rule in the chat says,
/// faint and on one line, so it reads as a mark between the prompts rather than one of them. It's never lit, since
/// it's never the one being read.
fn compaction_row(
    row: usize,
    compaction: &CompactionLine,
    jumps: &Jumps,
    line_text: &LineText,
    cx: &App,
) -> AnyElement {
    let fill = cx.theme().secondary_hover;
    let said = |to, between| -> SharedString {
        let freed = compaction::freed(compaction.before_tokens, compaction.after_tokens, to);
        let who = compaction::who(compaction.by);
        freed.map_or_else(|| who.into(), |freed| format!("{who}{between}{freed}").into())
    };
    // In words, since a screen reader would read the arrow out as one.
    let (label, said) = (said("to", ", "), said("→", " · "));
    // The compaction's icon, as its rule in the chat has it.
    let icon = Icon::new(IconName::FoldVertical).size(px(RULE_ICON)).text_color(look::faint());
    let icon = div().w(NUMBER_WIDTH).flex_none().flex().justify_end().items_center().child(icon);
    let text = div().flex_1().min_w_0().truncate().text_color(look::faint()).child(said);
    let text = line_text.small.set(text);
    div()
        .id(ElementId::Name(format!("outline-compaction/{row}").into()))
        .role(Role::ListBoxOption)
        .aria_label(label)
        .w(ROW_WIDTH)
        .flex()
        .items_center()
        .gap_3()
        .pl_1()
        .pr_3()
        .py_1p5()
        .rounded(px(8.))
        .hand()
        .hover(move |d| d.bg(fill))
        .on_click(jumps.to(row))
        .child(icon)
        .child(text)
        .test_support()
        .into_any_element()
}

/// What the prompt says, or for one that only carries files, how many.
fn label(line: &PromptLine) -> SharedString {
    if line.text().is_empty() {
        crate::calls::count(usize::try_from(line.files()).unwrap_or(usize::MAX), "file", "files").into()
    } else {
        line.text().to_string().into()
    }
}
