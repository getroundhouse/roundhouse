//! What a call opens to: a block under its line, holding code, output, or both, which scrolls once it's taller than
//! a few lines. Each text in it draws the matches of what's being found.

use std::borrow::Cow;

use gpui_kit::base::SelectableText;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, HighlightStyle, Hsla, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled, TestSupportExt as _, div, px,
};

use super::Toggle;
use super::highlight::spans;
use crate::attachments::{self, Carried};
use crate::find::{self, FindLook, Marks, Part, marked};
use crate::item_key::ItemKey;
use crate::log_block::{self, LogBlock, LogText};
use crate::long_texts::LongTexts;
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

/// A faint name for the part of a block below it, such as "Input".
pub(in crate::rows) fn label(text: impl Into<SharedString>, cx: &App) -> Div {
    let text: SharedString = text.into();
    faint(cx).child(text)
}

/// A faint name for the part of a block below it that's the agent's own, such as a file's path, which can be found.
pub(super) fn found_label(toggle: &Toggle, part: Part, text: &str, cx: &App) -> Div {
    faint(cx).child(marked(text.to_string().into(), &toggle.marks(part, text), &[], None))
}

fn faint(cx: &App) -> Div {
    div().px_3().pt_2().font_family(cx.theme().mono_font_family.clone()).text_xs().text_color(look::faint())
}

/// What [`output`] and [`lines`] draw of `text`: a short output without the blank lines at its end, and a long one
/// with every line cut short where [`LogBlock`] cuts it.
pub(in crate::rows) fn output_text(text: &str) -> Cow<'_, str> {
    if text.len() > LOG_BLOCK_ABOVE { log_block::shown(text) } else { Cow::Borrowed(text.trim_end()) }
}

/// Text a tool sent back, the call's `part`, selectable, scrolling past a dozen lines or so.
pub(in crate::rows) fn output(toggle: &Toggle, part: Part, text: &str, color: Hsla, cx: &App) -> AnyElement {
    let id = part.element(toggle.key);
    let area = scroll_id(&id);
    let shown = output_text(text);
    let marks = toggle.marks(part, &shown);
    let body = if text.len() > LOG_BLOCK_ABOVE {
        let font = cx.theme().mono_font_family.clone();
        LogBlock { log: LogText::new(&shown), color, font, marks, area: Some(area) }.into_any_element()
    } else {
        selectable(id.clone(), shown.into_owned(), &marks, area)
    };
    printed(toggle.find, &id, body, color, cx)
}

/// What a command printed, the call's `part`, in the foreground colour, with a little space between its lines, so one
/// too long for the block reads as one where it wraps. Each line is laid out with any blank lines after it, so a blank
/// line takes no more room than it did. Past [`SPACED_UP_TO`] lines, it's drawn as any output is.
pub(super) fn lines(toggle: &Toggle, part: Part, text: &str, cx: &App) -> AnyElement {
    let color = cx.theme().foreground;
    let Some(runs) = spaced(text) else { return output(toggle, part, text, color, cx) };
    let id = part.element(toggle.key);
    // Its lines a line apart are what [`output_text`] draws of it, which its matches are found in.
    let marks = toggle.marks(part, &output_text(text));
    let lines = runs.into_iter().map(|run| SharedString::from(run.to_string())).collect();
    let body = SelectableLines::new((id.clone(), "lines"), lines, px(LINE_GAP)).marked(marks, scroll_id(&id));
    printed(toggle.find, &id, body.into_any_element(), color, cx)
}

/// Text that came back, in `color` and the code font, in the space a block gives it, scrolling past a dozen lines or
/// so.
fn printed(find: &FindLook, id: &ElementId, body: AnyElement, color: Hsla, cx: &App) -> AnyElement {
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
    scrolling(find, id, part).into_any_element()
}

/// Text that can be selected, with its matches drawn behind it. The kit's selectable text draws no backgrounds, so
/// the same text is laid out under it, unseen but for them.
fn selectable(id: ElementId, text: String, marks: &Marks, area: ElementId) -> AnyElement {
    if marks.is_empty() {
        return SelectableText::new(id, text).into_any_element();
    }
    let under = marked(text.clone().into(), marks, &[], Some(area));
    div()
        .relative()
        .child(div().absolute().top_0().left_0().w_full().text_color(gpui_kit::transparent_black()).child(under))
        .child(SelectableText::new(id, text))
        .into_any_element()
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

/// Code in a block, the call's `part`, highlighted as `language`, after `prompt` in faint text. Its matches are found
/// in the code, not the prompt.
pub(super) fn code(toggle: &Toggle, part: Part, language: &str, prompt: &str, text: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let id = part.element(toggle.key);
    let body: SharedString = code_text(text).to_string().into();
    let spans = spans(language, &body);
    let marks = toggle.marks(part, &body);
    let shown: SharedString = format!("{prompt}{body}").into();
    let faint = HighlightStyle { color: Some(look::faint()), ..HighlightStyle::default() };
    let at = prompt.len();
    let styles: Vec<(std::ops::Range<usize>, HighlightStyle)> = (!prompt.is_empty())
        .then_some((0..at, faint))
        .into_iter()
        .chain(spans.iter().map(|(range, style)| (range.start + at..range.end + at, *style)))
        .collect();
    let part = div()
        .id(id.clone())
        .px_3()
        .py_2()
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .text_color(theme.foreground)
        .child(marked(shown, &marks.moved(at), &styles, Some(scroll_id(&id))))
        .test_support();
    scrolling(toggle.find, &id, part).into_any_element()
}

/// What [`code`] draws of `text`, after its prompt.
pub(in crate::rows) fn code_text(text: &str) -> &str {
    text.trim_end()
}

/// Prose in a block, the call's `part`, such as what a subagent was asked or what a model made of a page, as
/// Markdown, scrolling past a dozen lines or so, with its matches.
pub(super) fn prose(toggle: &Toggle, part: Part, text: &str) -> AnyElement {
    let id = part.element(toggle.key);
    let view = markdown(&toggle.shared.texts, toggle.key, part, text, toggle.find);
    let shown = div().id(id.clone()).px_3().py_2().text_sm().child(view).test_support();
    scrolling(toggle.find, &id, shown).into_any_element()
}

/// The item `key`'s `part`, Markdown, as [`prose`] draws it but without its padding or the area that scrolls it, with
/// the matches of what `find` looks for, which the view `texts` keeps of it draws.
pub(super) fn markdown(texts: &LongTexts, key: &ItemKey, part: Part, text: &str, find: &FindLook) -> AnyElement {
    let id = (key.clone(), part);
    let view = texts.markdown(find::view_id(&id), text);
    find::markdown_view(view, &id, find, Some(scroll_id(&part.element(key))))
}

/// A part of a block, such as its output, which keeps its own ID, in an area that scrolls it once it's taller than a
/// dozen lines or so, and scrolls to the match being shown in it. The area's ID comes from the part's, so each block
/// keeps its own place. At either end of the part, and over a part that fits, the wheel scrolls the transcript instead.
pub(in crate::rows) fn scrolling(find: &FindLook, id: &ElementId, part: impl IntoElement) -> ScrollArea {
    ScrollArea::new(scroll_id(id), px(MAX_HEIGHT), part).follows(find.reveal())
}

/// The ID of the area that scrolls the part `id`.
pub(super) fn scroll_id(id: &ElementId) -> ElementId {
    ElementId::from((id.clone(), "scroll"))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::{SPACED_UP_TO, output_text, runs, spaced};

    #[test]
    fn short_output_is_drawn_without_its_blank_end() {
        assert_eq!(output_text("ok\n\n"), "ok");
    }

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
