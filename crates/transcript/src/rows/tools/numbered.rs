//! Lines of a file in a block, each with its number in the file, highlighted for the file's language, and marked
//! when a change added or removed it, with the words that changed picked out.
//!
//! The diff panel draws with the kit's diff view, but that's a view of its own, which keeps its state between frames
//! and takes a whole diff's files. A call's few lines are drawn here instead, in the transcript's row, from what the
//! call and its result say.

use std::ops::Range;

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, HighlightStyle, Hsla, IntoElement, ParentElement, SharedString, Styled, StyledText,
    TestSupportExt as _, div, px,
};

use super::block;
use super::highlight::{Spans, spans};
use crate::calls::count;
use crate::calls::lines::{Mark, NumberedLine, Section};

/// More than a block is for. Past this, the rest of a file is left to the diff and the editor.
const SHOWN_LINES: usize = 300;
const LINE_HEIGHT: f32 = 20.0;
/// A dozen lines and half of the next, so a dozen fit whole and anything longer shows that it scrolls.
const MAX_HEIGHT: f32 = LINE_HEIGHT * 12.5 + 4.0;
const NUMBERS_WIDTH: f32 = 44.0;
const BAR_WIDTH: f32 = 3.0;
/// Colours for parts of a line, by where they are in it.
type Styles = Vec<(Range<usize>, HighlightStyle)>;
/// What a tab is drawn as, since the line's width is measured in spaces.
const TAB: &str = "    ";

/// The lines of `sections`, in a block that scrolls, with a gap between sections.
pub(super) fn lines(id: ElementId, language: &str, sections: &[Section], cx: &App) -> AnyElement {
    let (shown, more) = fitted(sections, SHOWN_LINES);
    let mut drawn = Vec::new();
    for (i, (section, shown)) in sections.iter().zip(shown).filter(|(_, shown)| *shown > 0).enumerate() {
        if i > 0 {
            drawn.push(gap());
        }
        drawn.extend(rows(language, &section[..shown], cx));
    }
    let more = (more > 0).then(|| format!("and {}", count(more, "more line", "more lines")));
    scrolling(id)
        .py_1()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(cx.theme().mono_font_size)
        .children(drawn)
        .when_some(more, |d, more| d.child(block::label(more, cx).pb_2()))
        .test_support()
        .into_any_element()
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

/// A block's numbered lines, scrolling past a dozen.
pub(super) fn scrolling(id: ElementId) -> gpui_kit::Stateful<gpui_kit::Div> {
    block::scrolling(id).max_h(px(MAX_HEIGHT))
}

/// A row for each of these lines, without a block around them.
pub(super) fn rows(language: &str, lines: &[NumberedLine], cx: &App) -> Vec<AnyElement> {
    let text: SharedString = lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n").into();
    let spans = spans(language, &text);
    let mut start = 0;
    lines
        .iter()
        .map(|line| {
            let range = start..start + line.text.len();
            start = range.end + 1;
            row(line, &styles_within(&spans, &range), cx)
        })
        .collect()
}

fn row(line: &NumberedLine, syntax: &[(Range<usize>, HighlightStyle)], cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (tint, numbers) = match line.mark {
        Mark::Same => (None, look::faint()),
        Mark::Added => (Some(theme.success), theme.success.opacity(0.75)),
        Mark::Removed => (Some(theme.danger), theme.danger.opacity(0.75)),
    };
    let (text, syntax, changed) = expand_tabs(&line.text, syntax, line.changed.clone());
    let text: SharedString = text.into();
    let styles = with_changed(&syntax, changed, tint.map(|t| t.opacity(0.3)), text.len());
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
                .min_w_0()
                .pr_3()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(theme.foreground)
                .child(StyledText::new(text).with_highlights(styles)),
        )
        .into_any_element()
}

/// A line with each tab drawn as spaces, and its colours and changed part moved to match. They're worked out on the
/// line as it is, so without moving they'd fall short of where they belong, and could cut a character in two.
fn expand_tabs(
    text: &str,
    syntax: &[(Range<usize>, HighlightStyle)],
    changed: Option<Range<usize>>,
) -> (String, Styles, Option<Range<usize>>) {
    if !text.contains('\t') {
        return (text.to_string(), syntax.to_vec(), changed);
    }
    let tabs_before =
        |offset: usize| text.char_indices().take_while(|(i, _)| *i < offset).filter(|(_, c)| *c == '\t').count();
    let at = |offset: usize| offset + tabs_before(offset) * (TAB.len() - 1);
    let moved = |range: &Range<usize>| at(range.start)..at(range.end);
    let syntax = syntax.iter().map(|(range, style)| (moved(range), *style)).collect();
    (text.replace('\t', TAB), syntax, changed.as_ref().map(moved))
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

/// The syntax colours, with a background added over the part of the line that changed. GPUI wants the pieces in
/// order and not overlapping, so they're cut at the changed part's edges.
fn with_changed(
    syntax: &[(Range<usize>, HighlightStyle)],
    changed: Option<Range<usize>>,
    background: Option<Hsla>,
    len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let (Some(changed), Some(background)) = (changed, background) else { return syntax.to_vec() };
    let changed = changed.start.min(len)..changed.end.min(len);
    let mut cuts = vec![0, len, changed.start, changed.end];
    cuts.extend(syntax.iter().flat_map(|(range, _)| [range.start, range.end]));
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter_map(|pair| {
            let piece = pair[0]..pair[1];
            let color = syntax.iter().find(|(range, _)| range.start <= piece.start && piece.end <= range.end);
            let mut style = color.map_or_else(HighlightStyle::default, |(_, style)| *style);
            let inside = changed.start <= piece.start && piece.end <= changed.end;
            if inside {
                style.background_color = Some(background);
            }
            (inside || color.is_some()).then_some((piece, style))
        })
        .collect()
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

        let (shown, syntax, changed) = expand_tabs(line, &[(string.clone(), HighlightStyle::default())], Some(string));

        assert_eq!(shown, "    let x = \"ééé\";");
        assert_eq!(&shown[syntax[0].0.clone()], "\"ééé\"", "the colour still covers the string, on whole characters");
        assert_eq!(&shown[changed.unwrap()], "\"ééé\"");
    }
}
