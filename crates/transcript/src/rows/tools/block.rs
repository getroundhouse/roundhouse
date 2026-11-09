//! What a call opens to: a block under its line, holding code, output, or both, which scrolls once it's taller than
//! a few lines.

use gpui_kit::base::SelectableText;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, HighlightStyle, Hsla, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, StyledText, TestSupportExt as _, div, px,
};

use super::highlight::spans;
use crate::attachments::{self, Carried};
use crate::item_key::ItemKey;
use crate::log_block::{LogBlock, LogText};

/// Above this, output goes through [`LogBlock`], which can hold anything but has no text selection yet.
const LOG_BLOCK_ABOVE: usize = 16 * 1024;
/// How tall a block's output gets before it scrolls: about a dozen lines.
const MAX_HEIGHT: f32 = 236.0;

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
pub(super) fn separator(cx: &App) -> Div {
    div().h(px(1.)).bg(cx.theme().border)
}

/// A faint name for the part of a block below it, such as "Input" or a file's path.
pub(super) fn label(text: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    let text: SharedString = text.into();
    div().px_3().pt_2().font_family(theme.mono_font_family.clone()).text_xs().text_color(look::faint()).child(text)
}

/// Text a tool sent back, selectable, scrolling past a dozen lines or so.
pub(in crate::rows) fn output(id: ElementId, text: &str, color: Hsla, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let font = theme.mono_font_family.clone();
    let body = if text.len() > LOG_BLOCK_ABOVE {
        LogBlock { log: LogText::new(text), color, font: font.clone() }.into_any_element()
    } else {
        SelectableText::new(id.clone(), text.trim_end().to_string()).into_any_element()
    };
    scrolling(id)
        .px_3()
        .py_2()
        .font_family(font)
        .text_size(theme.mono_font_size)
        .text_color(color)
        .child(body)
        .test_support()
        .into_any_element()
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
    scrolling(id)
        .px_3()
        .py_2()
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .text_color(theme.foreground)
        .child(StyledText::new(shown).with_highlights(styles.collect::<Vec<_>>()))
        .test_support()
        .into_any_element()
}

/// Prose in a block, such as what a subagent was asked or what a model made of a page, as Markdown, scrolling past
/// a dozen lines or so. It's the item's `part`.
pub(super) fn prose(key: &ItemKey, part: &str, text: &str) -> AnyElement {
    let text_id = key.element(&format!("{part}-text"));
    scrolling(key.element(part))
        .px_3()
        .py_2()
        .text_sm()
        .child(TextView::markdown(text_id, SharedString::from(text.to_string())).selectable(true))
        .test_support()
        .into_any_element()
}

pub(super) fn scrolling(id: ElementId) -> gpui_kit::Stateful<Div> {
    div().id(id).max_h(px(MAX_HEIGHT)).overflow_y_scroll()
}
