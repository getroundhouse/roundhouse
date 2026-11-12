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
use protocol::event::{ContentBlock, SessionEvent};
use protocol::model::{SubagentStatus, ToolUseId};

use super::line::{Line, Part};
use super::{Answer, Subagent, Toggle, block, line};
use crate::TranscriptEvent;
use crate::calls::agent::{self, Agent};
use crate::calls::count;
use crate::find::{self, FindLook, Part as Text};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::long_texts::LongTexts;
use crate::rows::around::{Around, blocks_of};
use crate::rows::shown::{self, Gather};
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
    if call.described().is_some() {
        line = line.found(toggle);
    }
    let opened = toggle.open.then(|| match subagent {
        Some(subagent) if subagent.apart => page(call, answer, subagent, toggle, cx),
        Some(_) | None => gathered(call, answer, subagent, toggle, cx),
    });
    line::drawn(line, answer, opened, toggle, cx)
}

/// What [`draw`] shows open, for finding: what it was asked, what it did, if that's gathered into it, and its report.
/// While its work is kept apart, what it's done lately, which changes as it works, isn't found.
pub(super) fn shown(
    call: &Agent,
    answer: Option<&Answer>,
    subagent: Option<&Subagent>,
    key: &ItemKey,
    gather: &mut Gather,
) {
    if let Some(title) = call.described() {
        gather.plain(key, Text::Title, title);
    }
    line::shown_failure(answer, false, key, gather);
    let events = subagent.and_then(|s| s.events.as_deref()).unwrap_or_default();
    gather.inside(key, Fold::Output, |gather| {
        if let Some(prompt) = call.prompt {
            gather.markdown(key, Text::Asked, prompt);
        }
        for (event, around) in events {
            for (block, drawing) in blocks_of(&event.body).iter().zip(&around.blocks) {
                shown::of_block(block, drawing, gather);
            }
        }
        let report = match subagent {
            Some(subagent) if subagent.apart => apart_report(answer, subagent),
            Some(_) | None => gathered_report(answer, events),
        };
        if let Some(report) = report {
            gather.markdown(key, Text::Report, report);
        }
    });
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
/// a block's prose does. The call and the subagent tab both start with it, with the matches of what `find` looks for.
pub(in crate::rows) fn prompt_block(
    texts: &LongTexts,
    key: &ItemKey,
    prompt: &str,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let id = Text::Asked.element(key);
    let text = div()
        .id(id.clone())
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(block::markdown(texts, key, Text::Asked, prompt, find))
        .test_support();
    v_flex()
        .mt_1()
        .gap_1()
        .pl_3()
        .border_l_2()
        .border_color(theme.border)
        .child(div().text_xs().text_color(look::faint()).child("Asked"))
        .child(block::scrolling(find, &id, text))
        .into_any_element()
}

/// What the call was asked, with its matches.
fn asked(call: &Agent, toggle: &Toggle, cx: &App) -> Option<AnyElement> {
    let prompt = call.prompt?;
    Some(prompt_block(&toggle.shared.texts, toggle.key, prompt, toggle.find, cx))
}

/// What it reported, `text`, once it's done, as the subagent's answer, as the item `key`'s: the agent's own prose, in
/// full, under a rule and a heading, so it carries more weight than what it was asked.
pub(in crate::rows) fn report_block(key: &ItemKey, text: impl IntoElement, cx: &App) -> Div {
    let theme = cx.theme();
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

/// The report of a subagent whose work is kept apart: its record's, once it's done. A subagent run in the foreground
/// reports in its call's result too, which is all there is until its record comes.
fn apart_report<'a>(answer: Option<&'a Answer>, subagent: &'a Subagent) -> Option<&'a str> {
    match &subagent.record {
        Some(record) if record.status == SubagentStatus::Working => None,
        Some(record) => record.report.as_deref(),
        None => answer.and_then(Answer::success).filter(|text| !agent::only_launched(text)),
    }
}

/// Kept apart: the blocks the subagent tab starts and ends with, and between them, while it works, what it's done lately:
/// its newest few calls, each after what it wrote just before it, then what it's written since.
fn page(call: &Agent, answer: Option<&Answer>, subagent: &Subagent, toggle: &Toggle, cx: &App) -> AnyElement {
    // It changes as the subagent works, so nothing's found in it.
    let unfound = FindLook::default();
    let drawn: Vec<AnyElement> = (subagent.lately.iter())
        .filter_map(|(block, drawing)| render_block(toggle.seq, block, drawing, toggle.shared, &unfound, cx))
        .collect();
    let lately = (!drawn.is_empty())
        .then(|| v_flex().gap_1().child(div().text_xs().text_color(look::faint()).child("Working on")).children(drawn));
    v_flex()
        .mt_1()
        .mb_2()
        .gap_2()
        .children(asked(call, toggle, cx))
        .children(lately)
        // A report names no images the hub kept, so only one written into it is drawn, and it loads in the call's row.
        .when_some(apart_report(answer, subagent), |d, report| {
            d.child(report_block(toggle.key, report_text(report, toggle), cx))
        })
        .into_any_element()
}

/// The report, as Markdown, with its matches, in the view the transcript keeps of it.
fn report_text(report: &str, toggle: &Toggle) -> AnyElement {
    let texts = &toggle.shared.texts;
    let view = agent_text(texts, toggle.key, report).image_source(toggle.shared.linked.resolver(toggle.seq, &[]));
    find::markdown_view(view, &(toggle.key.clone(), Text::Report), toggle.find, None)
}

/// The report of a subagent whose work is gathered into its call, if it shows it: a subagent run in the foreground
/// reports in its call's result, and its own words say the same, when they came.
fn gathered_report<'a>(answer: Option<&'a Answer>, events: &[(SessionEvent, Around)]) -> Option<&'a str> {
    let said = events
        .iter()
        .any(|(event, _)| blocks_of(&event.body).iter().any(|block| matches!(block, ContentBlock::Text { .. })));
    answer.and_then(Answer::success).filter(|text| !said && !agent::only_launched(text))
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
            .filter_map(|(block, drawing)| render_block(toggle.seq, block, drawing, toggle.shared, toggle.find, cx));
        work.extend(drawn);
    }
    let report = gathered_report(answer, events);
    // The rule is on the column's left edge, like everything else, and the work sits in from it to show it's nested.
    v_flex()
        .pl_3()
        .mt_1()
        .mb_2()
        .gap_2()
        .border_l_1()
        .border_color(theme.border)
        .children(asked(call, toggle, cx))
        .children(work)
        // A call's result names no images, so only one written into the report is drawn.
        .when_some(report, |d, report| d.child(report_text(report, toggle)))
        .into_any_element()
}
