//! A call that handed work to a subagent: what it's for, what kind of agent, and how many calls it's made. Opened,
//! what it was asked, then everything it did, under a rule, the way the transcript draws the agent's own work.
//! [`crate::rows::around`] gathers that work from the log.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use protocol::event::ContentBlock;

use super::line::{HEIGHT, Line, Part};
use super::{Answer, Subagent, Toggle, block, line};
use crate::TranscriptEvent;
use crate::calls::agent::{self, Agent};
use crate::calls::count;
use crate::rows::around::blocks_of;
use crate::rows::{agent_text, render_block};
use crate::sketch::{Sketch, sketch};

pub(super) fn draw(
    call: &Agent,
    answer: Option<&Answer>,
    subagent: Option<&Subagent>,
    toggle: &Toggle,
    cx: &App,
) -> AnyElement {
    let calls = subagent.map_or(0, |s| s.calls);
    let mut line = Line::new(IconName::Bot, call.title())
        .part(call.kind.map(|kind| Part::Chip(kind.to_string().into())))
        .part((calls > 0).then(|| Part::Note(count(calls, "call", "calls").into())));
    if sketch() == Sketch::Page
        && let Some(subagent) = subagent
    {
        let open = Button::new(open_id(&subagent.call))
            .ghost()
            .xsmall()
            .icon(IconName::ArrowUpRight)
            .tooltip("Open in tab")
            .on_click(open_tab(&subagent.call, call.title(), toggle));
        line = line.action(open.into_any_element());
    }
    let opened = toggle.open.then(|| match sketch() {
        Sketch::Chat => chat(call, answer, subagent, toggle, cx),
        Sketch::Page => page(call, answer, subagent, toggle, cx),
        Sketch::Thread => thread(call, answer, subagent, toggle, cx),
    });
    line::drawn(line, answer, opened, toggle, cx)
}

fn open_id(call: &str) -> ElementId {
    ElementId::Name(format!("open-subagent-{call}").into())
}

/// Asks for the subagent's work in the subagent tab.
fn open_tab(
    call: &str,
    title: &str,
    toggle: &Toggle,
) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut App) + use<> {
    let (this, call, title) = (toggle.this.clone(), call.to_string(), title.to_string());
    move |_, _, cx| {
        // The line under it would take the click too, and open or close the call.
        cx.stop_propagation();
        let (call, title) = (call.clone().into(), title.clone());
        let _ = this.update(cx, |_, cx| cx.emit(TranscriptEvent::OpenSubagent { call, title }));
    }
}

/// What it was asked, on a block labelled as a call's input is.
fn prompt_block(call: &Agent, toggle: &Toggle, cx: &App) -> Option<Div> {
    let prompt = call.prompt?;
    Some(block::frame(cx).child(block::label("Prompt", cx)).child(block::prose(toggle.key, "prompt", prompt)))
}

/// Its newest call, drawn as any call is, while it works.
fn latest(subagent: Option<&Subagent>, toggle: &Toggle, cx: &App) -> Option<AnyElement> {
    let subagent = subagent.filter(|s| s.ended.is_none())?;
    let events = subagent.events.as_deref()?;
    events.iter().rev().find_map(|(event, around)| {
        let blocks = blocks_of(&event.body).iter().zip(&around.blocks).rev();
        blocks
            .filter(|(block, _)| matches!(block, ContentBlock::ToolUse { .. }))
            .find_map(|(block, drawing)| render_block(toggle.seq, block, drawing, toggle.this, toggle.linked, cx))
    })
}

/// What it reported, once it's done. A subagent run in the foreground reports in its call's result.
fn report(answer: Option<&Answer>, subagent: Option<&Subagent>) -> Option<String> {
    if let Some(ended) = subagent.and_then(|s| s.ended.as_ref()) {
        return ended.report.clone();
    }
    answer.and_then(Answer::success).filter(|text| !agent::only_launched(text)).map(str::to_string)
}

/// The chat's option: what it was asked on a block, then its newest call or its report, then a button that opens
/// its work in the tab, outlined as a button that opens something is.
fn chat(call: &Agent, answer: Option<&Answer>, subagent: Option<&Subagent>, toggle: &Toggle, cx: &App) -> AnyElement {
    let open = subagent.map(|s| {
        Button::new(open_id(&s.call))
            .outline()
            .small()
            .label("Open in tab")
            .child(Icon::new(IconName::ArrowUpRight).xsmall())
            .on_click(open_tab(&s.call, call.title(), toggle))
    });
    v_flex()
        .mt_1()
        .mb_2()
        .gap_2()
        .children(prompt_block(call, toggle, cx))
        .children(latest(subagent, toggle, cx))
        .when_some(report(answer, subagent), |d, report| {
            d.child(agent_text(toggle.key, &report, None).image_source(toggle.linked.resolver(toggle.seq)))
        })
        .when_some(open, |d, open| d.child(h_flex().pt_1().child(open)))
        .into_any_element()
}

/// The page's option: the same blocks the tab starts and ends with, what it was asked and its report, with its
/// newest call between them while it works. The line's button opens the tab.
fn page(call: &Agent, answer: Option<&Answer>, subagent: Option<&Subagent>, toggle: &Toggle, cx: &App) -> AnyElement {
    let latest = latest(subagent, toggle, cx)
        .map(|latest| v_flex().child(div().text_xs().text_color(look::faint()).child("Working on")).child(latest));
    let report = report(answer, subagent).map(|report| {
        block::frame(cx)
            .child(block::label("Report", cx))
            .child(div().px_3().py_2().child(agent_text(toggle.key, &report, None)))
    });
    v_flex().children(prompt_block(call, toggle, cx)).children(latest).children(report).into_any_element()
}

/// The thread's option: under a rule down its left, what it was asked, muted, then its newest call or its report,
/// then a line like a call's that opens the rest in the tab.
fn thread(call: &Agent, answer: Option<&Answer>, subagent: Option<&Subagent>, toggle: &Toggle, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let asked =
        call.prompt.map(|prompt| div().text_color(theme.muted_foreground).child(agent_text(toggle.key, prompt, None)));
    let calls = subagent.map_or(0, |s| s.calls);
    let open = subagent.map(|s| {
        let hover = theme.muted;
        h_flex()
            .id(open_id(&s.call))
            .h(px(HEIGHT))
            .mx(px(-8.))
            .px(px(8.))
            .gap_2()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(move |d| d.bg(hover))
            .text_color(theme.muted_foreground)
            .on_click(open_tab(&s.call, call.title(), toggle))
            .child(Icon::new(IconName::PanelRightOpen).size(px(15.)).text_color(look::faint()))
            .child(format!("See all {}", count(calls, "call", "calls")))
            .child(Icon::new(IconName::ChevronRight).size(px(15.)).text_color(look::faint()))
            .test_support()
    });
    v_flex()
        .pl_3()
        .mt_1()
        .mb_2()
        .gap_2()
        .border_l_1()
        .border_color(theme.border)
        .children(asked)
        .children(latest(subagent, toggle, cx))
        .when_some(report(answer, subagent), |d, report| {
            d.child(agent_text(toggle.key, &report, None).image_source(toggle.linked.resolver(toggle.seq)))
        })
        .children(open)
        .into_any_element()
}
