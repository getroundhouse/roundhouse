//! What a call opens to: a block under its line, holding code, output, or both, which scrolls once it's taller than
//! a few lines.

use gpui_kit::base::SelectableText;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, HighlightStyle, Hsla, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled, StyledText, TestSupportExt as _, div, px,
};

use super::highlight::spans;
use crate::attachments::{self, Carried};
use crate::item_key::ItemKey;
use crate::log_block::{LogBlock, LogText};
use crate::scroll_area::ScrollArea;
use crate::selectable_lines::SelectableLines;

/// Above this, output goes through [`LogBlock`], which can hold anything but has no text selection yet.
const LOG_BLOCK_ABOVE: usize = 16 * 1024;
/// How tall a block's output gets before it scrolls: about a dozen lines.
const MAX_HEIGHT: f32 = 236.0;
/// The space between the lines of what a command printed, so a line that wraps reads as one: about a fifth of a line.
const LINE_GAP: f32 = 4.0;
/// Up to how many lines a command's output is drawn a line at a time, with space between. Each line is a text of its
/// own, which costs about 3 µs on every frame its row is on screen, so a longer log is drawn as one text, without it.
const SPACED_UP_TO: usize = 200;

/// The block itself, on the column's left edge with the call's icon, as everything in the transcript is.
pub(in crate::rows) fn frame(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .mt_1()
        .mb_2()
        .flex()
        .flex_col()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .overflow_hidden()
}

/// A line between the parts of a block, such as a command and its output.
pub(in crate::rows) fn separator(cx: &App) -> Div {
    div().h(px(1.)).bg(cx.theme().border)
}

/// A faint name for the part of a block below it, such as "Input" or a file's path.
pub(in crate::rows) fn label(text: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    let text: SharedString = text.into();
    div().px_3().pt_2().font_family(theme.mono_font_family.clone()).text_xs().text_color(look::faint()).child(text)
}

/// Text a tool sent back, selectable, scrolling past a dozen lines or so.
pub(in crate::rows) fn output(id: ElementId, text: &str, color: Hsla, cx: &App) -> AnyElement {
    let body = if text.len() > LOG_BLOCK_ABOVE {
        LogBlock { log: LogText::new(text), color, font: cx.theme().mono_font_family.clone() }.into_any_element()
    } else {
        SelectableText::new(id.clone(), text.trim_end().to_string()).into_any_element()
    };
    printed(id, body, color, cx)
}

/// What a command printed, in the foreground colour, with a little space between its lines, so one too long for the
/// block reads as one where it wraps. Each line is laid out with any blank lines after it, so a blank line takes no
/// more room than it did. Past [`SPACED_UP_TO`] lines, it's drawn as any output is.
pub(super) fn lines(id: ElementId, text: &str, cx: &App) -> AnyElement {
    let color = cx.theme().foreground;
    let Some(runs) = spaced(text) else { return output(id, text, color, cx) };
    let lines = runs.into_iter().map(|run| SharedString::from(run.to_string())).collect();
    let body = SelectableLines::new((id.clone(), "lines"), lines, px(LINE_GAP)).into_any_element();
    printed(id, body, color, cx)
}

/// Text that came back, in `color` and the code font, in the space a block gives it, scrolling past a dozen lines or so.
fn printed(id: ElementId, body: AnyElement, color: Hsla, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let part = div()
        .id(id.clone())
        .px_3()
        .py_2()
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .text_color(color)
        .child(body)
        .test_support();
    scrolling(id, part).into_any_element()
}

/// The text's lines, as [`runs`] gives them, if there are few enough to draw one at a time. A text too long for that
/// is turned away before it's read line by line, since this is asked on every frame its call is open.
fn spaced(text: &str) -> Option<Vec<&str>> {
    if text.len() > LOG_BLOCK_ABOVE {
        return None;
    }
    let runs = runs(text.trim_end());
    (runs.len() <= SPACED_UP_TO).then_some(runs)
}

/// The text's lines, each with the blank lines that follow it, and the first with any before it, so only a text
/// that's all blank has a run with nothing in it.
fn runs(text: &str) -> Vec<&str> {
    let (mut runs, mut start, mut at) = (Vec::new(), 0, 0);
    for line in text.split_inclusive('\n') {
        if !line.trim().is_empty() && !text[start..at].trim().is_empty() {
            // Without the newline that ends it, which would draw an empty line under it.
            runs.push(&text[start..at - 1]);
            start = at;
        }
        at += line.len();
    }
    runs.push(&text[start..]);
    runs
}

/// Images a tool sent back, each whole, one under another.
pub(in crate::rows) fn images(images: &[Carried], cx: &App) -> AnyElement {
    v_flex().px_3().py_2().gap_2().children(images.iter().map(|image| attachments::whole(image, cx))).into_any_element()
}

/// Code in a block, highlighted as `language`, after `prompt` in faint text.
pub(super) fn code(id: ElementId, language: &str, prompt: &str, text: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let body: SharedString = text.trim_end().to_string().into();
    let spans = spans(language, &body);
    let shown: SharedString = format!("{prompt}{body}").into();
    let faint = HighlightStyle { color: Some(look::faint()), ..HighlightStyle::default() };
    let prompt_style = (!prompt.is_empty()).then_some((0..prompt.len(), faint));
    let at = prompt.len();
    let styles =
        prompt_style.into_iter().chain(spans.iter().map(|(range, style)| (range.start + at..range.end + at, *style)));
    let part = div()
        .id(id.clone())
        .px_3()
        .py_2()
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .text_color(theme.foreground)
        .child(StyledText::new(shown).with_highlights(styles.collect::<Vec<_>>()))
        .test_support();
    scrolling(id, part).into_any_element()
}

/// Prose in a block, such as what a subagent was asked or what a model made of a page, as Markdown, scrolling past
/// a dozen lines or so. It's the item's `part`.
pub(in crate::rows) fn prose(key: &ItemKey, part: &str, text: &str) -> AnyElement {
    let (id, text_id) = (key.element(part), key.element(&format!("{part}-text")));
    let shown = div()
        .id(id.clone())
        .px_3()
        .py_2()
        .text_sm()
        .child(TextView::markdown(text_id, SharedString::from(text.to_string())).selectable(true))
        .test_support();
    scrolling(id, shown).into_any_element()
}

/// A part of a block, such as its output, which keeps its own ID, in an area that scrolls it once it's taller than a
/// dozen lines or so. The area's ID comes from the part's, so each block keeps its own place. At either end of the
/// part, and over a part that fits, the wheel scrolls the transcript instead.
pub(in crate::rows) fn scrolling(id: ElementId, part: impl IntoElement) -> ScrollArea {
    ScrollArea::new((id, "scroll"), px(MAX_HEIGHT), part)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::{SPACED_UP_TO, runs, spaced};

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        /// Output drawn from its runs, a line apart, is the output again, and a run with nothing in it is only ever a
        /// text that's all blank.
        #[test]
        fn output_drawn_from_its_runs_is_the_output_again(text in "[a \t\r\n]{0,40}") {
            let drawn = runs(&text);

            prop_assert_eq!(drawn.join("\n"), text.clone());
            prop_assert!(drawn.iter().all(|run| !run.trim().is_empty()) || text.trim().is_empty());
        }
    }

    #[test]
    fn each_line_of_output_keeps_the_blank_lines_after_it() {
        assert_eq!(runs("one\ntwo"), ["one", "two"]);
        assert_eq!(runs("one\n\n  \ntwo\nthree"), ["one\n\n  ", "two", "three"]);
        assert_eq!(runs("\n\none\ntwo"), ["\n\none", "two"], "the first takes any blank lines before it");
        assert_eq!(runs("   Compiling\n    Finished"), ["   Compiling", "    Finished"]);
    }

    #[test]
    fn output_is_drawn_a_line_at_a_time_only_while_it_s_short() {
        let lines = |count: usize| (1..=count).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");

        assert!(spaced(&lines(SPACED_UP_TO)).is_some());
        assert!(spaced(&lines(SPACED_UP_TO + 1)).is_none(), "a long log is drawn as one text");
        assert!(spaced(&"x".repeat(20 * 1024)).is_none(), "and so is a long line");
    }
}
