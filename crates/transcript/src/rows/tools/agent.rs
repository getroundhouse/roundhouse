//! A call that handed work to a subagent: what it's for, what kind of agent unless it's the default, and how many
//! calls it's made.
//!
//! When the hub keeps the subagent's work apart, it's drawn from the subagent's record. Opened, it shows the blocks
//! the subagent tab starts and ends with: what it was asked, and its report once it's done, with its newest call
//! between them while it works. A button at the line's end opens the tab.
//!
//! From a hub that doesn't, opened, it shows what it was asked, then everything it did, under a rule, the way the
//! transcript draws the agent's own work. [`crate::rows::around`] gathers that work from the log.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Styled,
    TestSupportExt as _, div,
};
use look::Hand as _;
use protocol::event::ContentBlock;
use protocol::model::{Seq, SubagentStatus, ToolUseId};

use super::line::{Line, Part};
use super::{Answer, Subagent, Toggle, block, line};
use crate::TranscriptEvent;
use crate::calls::agent::{self, Agent};
use crate::calls::count;
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::long_texts::LongTexts;
use crate::rows::around::blocks_of;
use crate::rows::{agent_text, render_block};

pub(super) fn draw(
    call: &Agent,
    answer: Option<&Answer>,
    subagent: Option<&Subagent>,
    toggle: &Toggle,
    cx: &App,
) -> AnyElement {
    let mut line = Line::new(IconName::Bot, call.title())
        .part(agent::shown_kind(call.kind).map(|kind| Part::Chip(kind.to_string().into())))
        .part(standing(subagent).map(|standing| Part::Note(standing.into())));
    // The tab reads the subagent's work by its call, which only a hub that keeps it apart can, whether or not its
    // record has come, or ever will, as for one recorded before the agent said when its subagents started.
    if let Some(subagent) = subagent.filter(|s| s.apart) {
        line = line.action(open_tab(subagent.call.clone(), call.title(), toggle));
    }
    let opened = toggle.open.then(|| match subagent {
        Some(subagent) if subagent.apart => page(call, answer, subagent, toggle, cx),
        Some(_) | None => gathered(call, answer, subagent, toggle, cx),
    });
    line::drawn(line, answer, opened, toggle, cx)
}

/// How many calls it's made, and how it ended if that wasn't by finishing.
fn standing(subagent: Option<&Subagent>) -> Option<String> {
    let subagent = subagent?;
    let calls = (subagent.calls > 0).then(|| count(subagent.calls, "call", "calls"));
    let ended = subagent.record.as_ref().and_then(|record| match record.status {
        SubagentStatus::Failed => Some("Failed"),
        SubagentStatus::Stopped => Some("Stopped"),
        SubagentStatus::Working | SubagentStatus::Finished | SubagentStatus::Unknown => None,
    });
    let words: Vec<String> = ended.map(str::to_string).into_iter().chain(calls).collect();
    (!words.is_empty()).then(|| words.join(" · "))
}

/// The button that asks for the subagent's work in the subagent tab. It says Open, and it's outlined, as a button that
/// opens something is, so it reads as the way in, apart from the line, whose click opens the call where it is.
fn open_tab(call: ToolUseId, title: &str, toggle: &Toggle) -> AnyElement {
    let (this, title) = (toggle.shared.this.clone(), title.to_string());
    Button::new(ElementId::Name(format!("open-subagent/{call}").into()))
        .hand()
        .outline()
        .xsmall()
        .label("Open")
        .icon(IconName::ArrowUpRight)
        .tooltip("Open in the subagent tab")
        .on_click(move |_, _, cx| {
            // The line under it would take the click too, and open or close the call.
            cx.stop_propagation();
            let (call, title) = (call.clone(), title.clone());
            let _ = this.update(cx, |_, cx| cx.emit(TranscriptEvent::OpenSubagent { call, title }));
        })
        .into_any_element()
}

/// What it was asked, quieter than what it reported, which is the outcome: a quote, with a rule at its left and its
/// text fainter, under the word Asked, since a prompt is what a person sends. It scrolls past a dozen lines or so, as
/// a block's prose does. The call and the subagent tab both start with it.
pub(in crate::rows) fn prompt_block(texts: &LongTexts, key: &ItemKey, prompt: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (id, text_id) = (key.element("prompt"), key.element("prompt-text"));
    let text = div()
        .id(id.clone())
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(texts.markdown(text_id, prompt))
        .test_support();
    v_flex()
        .mt_1()
        .gap_1()
        .pl_3()
        .border_l_2()
        .border_color(theme.border)
        .child(div().text_xs().text_color(look::faint()).child("Asked"))
        .child(block::scrolling(id, text))
        .into_any_element()
}

/// What it reported, once it's done, as the subagent's answer: the agent's own prose, in full, under a rule and a
/// heading, so it carries more weight than what it was asked. A subagent run in the foreground reports in its call's
/// result too, which is all there is until its record comes. A report names no images the hub kept, so only one
/// written into it is drawn, and it loads in the row of the event at `seq`, when it's drawn in one, as it is in the
/// chat.
pub(in crate::rows) fn report_block(
    texts: &LongTexts,
    key: &ItemKey,
    report: &str,
    images: Option<(&LinkedImages, Seq)>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let text = agent_text(texts, key, report);
    let text = match images {
        Some((linked, seq)) => text.image_source(linked.resolver(seq, &[])),
        None => text,
    };
    let heading = div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child("Report");
    v_flex()
        .mt_1()
        .gap_2()
        .pt_3()
        .border_t_1()
        .border_color(theme.border)
        .child(heading)
        .child(div().id(key.element("report")).child(text).test_support())
}

/// Kept apart: the blocks the subagent tab starts and ends with, and between them, while it works, what it's done lately:
/// its newest few calls, each after what it wrote just before it, then what it's written since.
fn page(call: &Agent, answer: Option<&Answer>, subagent: &Subagent, toggle: &Toggle, cx: &App) -> AnyElement {
    let drawn: Vec<AnyElement> = (subagent.lately.iter())
        .filter_map(|(block, drawing)| render_block(toggle.seq, block, drawing, toggle.shared, cx))
        .collect();
    let lately = (!drawn.is_empty())
        .then(|| v_flex().gap_1().child(div().text_xs().text_color(look::faint()).child("Working on")).children(drawn));
    let report = match &subagent.record {
        Some(record) if record.status == SubagentStatus::Working => None,
        Some(record) => record.report.as_deref(),
        None => answer.and_then(Answer::success).filter(|text| !agent::only_launched(text)),
    };
    v_flex()
        .mt_1()
        .mb_2()
        .gap_2()
        .children(call.prompt.map(|prompt| prompt_block(&toggle.shared.texts, toggle.key, prompt, cx)))
        .children(lately)
        .when_some(report, |d, report| {
            d.child(report_block(
                &toggle.shared.texts,
                toggle.key,
                report,
                Some((&toggle.shared.linked, toggle.seq)),
                cx,
            ))
        })
        .into_any_element()
}

/// Gathered from the log: what it was asked, then what it did, under a rule like a thread.
fn gathered(
    call: &Agent,
    answer: Option<&Answer>,
    subagent: Option<&Subagent>,
    toggle: &Toggle,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let events = subagent.and_then(|s| s.events.as_deref()).unwrap_or_default();
    let mut work = Vec::new();
    for (event, around) in events {
        // Drawn in the call's row, so opening one of its calls makes this row taller.
        let drawn = blocks_of(&event.body)
            .iter()
            .zip(&around.blocks)
            .filter_map(|(block, drawing)| render_block(toggle.seq, block, drawing, toggle.shared, cx));
        work.extend(drawn);
    }
    // A subagent run in the foreground reports in its call's result. Its own words say the same, when they came.
    let said = events
        .iter()
        .any(|(event, _)| blocks_of(&event.body).iter().any(|block| matches!(block, ContentBlock::Text { .. })));
    let report = answer.and_then(Answer::success).filter(|text| !said && !agent::only_launched(text));
    // The rule is on the column's left edge, like everything else, and the work sits in from it to show it's nested.
    v_flex()
        .pl_3()
        .mt_1()
        .mb_2()
        .gap_2()
        .border_l_1()
        .border_color(theme.border)
        .children(call.prompt.map(|prompt| prompt_block(&toggle.shared.texts, toggle.key, prompt, cx)))
        .children(work)
        .when_some(report, |d, report| {
            // A call's result names no images, so only one written into the report is drawn.
            d.child(
                agent_text(&toggle.shared.texts, toggle.key, report)
                    .image_source(toggle.shared.linked.resolver(toggle.seq, &[])),
            )
        })
        .into_any_element()
}
