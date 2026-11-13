//! Lines of a file in a block, each with its number in the file, highlighted for the file's language, and marked
//! when a change added or removed it, with the words that changed picked out. A line is never cut short or wrapped,
//! so the block scrolls sideways to the end of its longest, as the file tab does.
//!
//! The diff panel draws with the kit's diff view, but that's a view of its own, which keeps its state between frames
//! and takes a whole diff's files. A call's few lines are drawn here instead, in the transcript's row, from what the
//! call and its result say.

use std::ops::Range;

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, HighlightStyle, Hsla, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, Styled, TestSupportExt as _, div, px,
};

use super::highlight::{Spans, spans};
use super::{Toggle, block};
use crate::calls::count;
use crate::calls::lines::{Mark, NumberedLine, Section};
use crate::find::{self, Marks, Part, behind};
use crate::log_block::{TAB, tabbed};
use crate::scroll_area::ScrollArea;

/// More than a block is for. Past this, the rest of a file is left to the diff and the editor.
const SHOWN_LINES: usize = 300;
const LINE_HEIGHT: f32 = 20.0;
/// A dozen lines and half of the next, so a dozen fit whole and anything longer shows that it scrolls.
const MAX_HEIGHT: f32 = LINE_HEIGHT * 12.5 + 4.0;
const NUMBERS_WIDTH: f32 = 44.0;
const BAR_WIDTH: f32 = 3.0;
/// Colours for parts of a line, by where they are in it.
type Styles = Vec<(Range<usize>, HighlightStyle)>;

/// The lines of `sections`, the call's `part`, in a block that scrolls, with a gap between sections.
pub(super) fn lines(toggle: &Toggle, part: Part, language: &str, sections: &[Section], cx: &App) -> AnyElement {
    let id = part.element(toggle.key);
    let area = block::scroll_id(&id);
    let marks = toggle.find.marks_of(&(toggle.key.clone(), part), || shown_text(sections).into());
    let (shown, more) = fitted(sections, SHOWN_LINES);
    let (mut drawn, mut at) = (Vec::new(), 0);
    for (i, (section, shown)) in sections.iter().zip(shown).filter(|(_, shown)| *shown > 0).enumerate() {
        if i > 0 {
            drawn.push(gap());
        }
        let lines = &section[..shown];
        let len = joined_len(lines);
        drawn.extend(rows(language, lines, &marks.within(at, at + len), Some(&area), cx));
        at += len + 1;
    }
    let more = (more > 0).then(|| format!("and {}", count(more, "more line", "more lines")));
    let part = div()
        .id(id.clone())
        .py_1()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(cx.theme().mono_font_size)
        .children(drawn)
        .when_some(more, |d, more| d.child(block::label(more, cx).pb_2()))
        .test_support();
    scrolling(toggle, &id, part).into_any_element()
}

/// What [`lines`] draws of `sections`, for finding: each line it shows, one after another.
pub(super) fn shown_text(sections: &[Section]) -> String {
    let (shown, _) = fitted(sections, SHOWN_LINES);
    let lines = sections.iter().zip(shown).flat_map(|(section, shown)| &section[..shown]);
    lines.map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
}

/// What [`rows`] draws of `lines`, for finding.
pub(super) fn rows_text(lines: &[NumberedLine]) -> String {
    lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
}

/// How long `lines` are, with a line break between each.
fn joined_len(lines: &[NumberedLine]) -> usize {
    lines.iter().map(|line| line.text.len()).sum::<usize>() + lines.len().saturating_sub(1)
}

/// How many of each section's lines fit in `room` lines, in order, and how many are left out.
fn fitted(sections: &[Section], room: usize) -> (Vec<usize>, usize) {
    let mut left = room;
    let shown = sections
        .iter()
        .map(|section| {
            let shown = section.len().min(left);
            left -= shown;
            shown
        })
        .collect();
    let total: usize = sections.iter().map(Vec::len).sum();
    (shown, total - (room - left))
}

/// A block's numbered lines, `part`, scrolling past a dozen, and sideways along a line too long for it.
pub(super) fn scrolling(toggle: &Toggle, id: &ElementId, part: impl IntoElement) -> ScrollArea {
    block::scrolling(toggle.find, &toggle.shared.selection, id, part).max_height(px(MAX_HEIGHT)).both_ways()
}

/// A row for each of these lines, without a block around them, with `marks`, which are where the matches are in
/// [`rows_text`], in `area` if it's an area that scrolls. A line's text is never cut short, so the rows go in an area
/// that holds them as wide as the longest, as [`scrolling`] and the edit's preview do.
pub(super) fn rows(
    language: &str,
    lines: &[NumberedLine],
    marks: &Marks,
    area: Option<&ElementId>,
    cx: &App,
) -> Vec<AnyElement> {
    let text: SharedString = rows_text(lines).into();
    let spans = spans(language, &text);
    let mut start = 0;
    lines
        .iter()
        .map(|line| {
            let range = start..start + line.text.len();
            start = range.end + 1;
            let marks = marks.within(range.start, range.end);
            row(line, &styles_within(&spans, &range), &marks, area.cloned(), cx)
        })
        .collect()
}

fn row(
    line: &NumberedLine,
    syntax: &[(Range<usize>, HighlightStyle)],
    marks: &Marks,
    area: Option<ElementId>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let (tint, numbers) = match line.mark {
        Mark::Same => (None, look::faint()),
        Mark::Added => (Some(theme.success), theme.success.opacity(0.75)),
        Mark::Removed => (Some(theme.danger), theme.danger.opacity(0.75)),
    };
    let styles = with_changed(syntax, &line.changed, tint.map(|t| t.opacity(0.3)), line.text.len());
    let styles = marks.over(&styles, line.text.len());
    let here =
        marks.here().zip(marks.reveal()).map(|(range, reveal)| (tabbed(&line.text, range.start), reveal.clone()));
    let (text, styles) = expand_tabs(&line.text, &styles);
    let text: SharedString = text.into();
    h_flex()
        .h(px(LINE_HEIGHT))
        .flex_none()
        .border_l(px(BAR_WIDTH))
        .border_color(tint.unwrap_or(gpui_kit::transparent_black()))
        .when_some(tint, |d, tint| d.bg(tint.opacity(0.11)))
        .child(
            div()
                .flex_none()
                .w(px(NUMBERS_WIDTH))
                .pr_3()
                .text_right()
                .text_color(numbers)
                .children(line.number.map(|n| n.to_string())),
        )
        .child(
            div()
                .flex_none()
                .pr_3()
                .whitespace_nowrap()
                .text_color(theme.foreground)
                .child(find::styled(text, styles, here, area)),
        )
        .into_any_element()
}

/// A line with each tab drawn as spaces, and its colours moved to match. They're worked out on the line as it is, so
/// without moving they'd fall short of where they belong, and could cut a character in two.
fn expand_tabs(text: &str, styles: &[(Range<usize>, HighlightStyle)]) -> (String, Styles) {
    if !text.contains('\t') {
        return (text.to_string(), styles.to_vec());
    }
    let moved = |range: &Range<usize>| tabbed(text, range.start)..tabbed(text, range.end);
    let styles = styles.iter().map(|(range, style)| (moved(range), *style)).collect();
    (text.replace('\t', TAB), styles)
}

/// The faint mark between two sections of a file that aren't next to each other, where their numbers go.
fn gap() -> AnyElement {
    h_flex()
        .h(px(LINE_HEIGHT))
        .border_l(px(BAR_WIDTH))
        .border_color(gpui_kit::transparent_black())
        .child(div().w(px(NUMBERS_WIDTH)).pr_3().text_right().text_color(look::faint()).child("⋯"))
        .into_any_element()
}

/// The syntax colours that fall within `range` of the joined text, moved to start at the line.
fn styles_within(spans: &Spans, range: &Range<usize>) -> Vec<(Range<usize>, HighlightStyle)> {
    spans
        .iter()
        .filter(|(span, _)| span.start < range.end && span.end > range.start)
        .map(|(span, style)| (span.start.max(range.start) - range.start..span.end.min(range.end) - range.start, *style))
        .filter(|(span, _)| !span.is_empty())
        .collect()
}

/// The syntax colours, with a background added over the words that changed.
fn with_changed(
    syntax: &[(Range<usize>, HighlightStyle)],
    changed: &[Range<usize>],
    background: Option<Hsla>,
    len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some(background) = background.filter(|_| !changed.is_empty()) else { return syntax.to_vec() };
    let backgrounds: Vec<_> =
        changed.iter().map(|range| (range.start.min(len)..range.end.min(len), background)).collect();
    behind(syntax, &backgrounds, len)
}

#[cfg(test)]
mod tests {
    use gpui_kit::HighlightStyle;

    use super::{expand_tabs, fitted};
    use crate::calls::lines::{NumberedLine, Section};

    #[test]
    fn what_doesn_t_fit_is_counted_by_its_lines() {
        let section = |n: u32| -> Section { (1..=n).map(|number| NumberedLine::same(number, "x")).collect() };

        assert_eq!(fitted(&[section(200), section(50), section(100)], 300), (vec![200, 50, 50], 50));
        assert_eq!(fitted(&[section(3), section(4)], 300), (vec![3, 4], 0));
    }

    #[test]
    fn a_tab_moves_what_comes_after_it_to_where_it_is_drawn() {
        let line = "\tlet x = \"ééé\";";
        let string = line.find('"').unwrap()..line.rfind('"').unwrap() + 1;

        let (shown, styles) = expand_tabs(line, &[(string, HighlightStyle::default())]);

        assert_eq!(shown, "    let x = \"ééé\";");
        assert_eq!(&shown[styles[0].0.clone()], "\"ééé\"", "the colour still covers the string, on whole characters");
    }
}
