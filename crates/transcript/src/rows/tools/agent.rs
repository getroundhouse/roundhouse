//! A call that handed work to a subagent: what it's for, what kind of agent, and how many calls it's made. Opened,
//! what it was asked, then everything it did, under a rule, the way the transcript draws the agent's own work.
//! [`crate::rows::around`] gathers that work from the log.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled};
use protocol::event::ContentBlock;

use super::line::{Line, Part};
use super::{Answer, Subagent, Toggle, block, line};
use crate::calls::agent::{self, Agent};
use crate::calls::count;
use crate::rows::around::blocks_of;
use crate::rows::{agent_text, render_block};

pub(super) fn draw(
    call: &Agent,
    answer: Option<&Answer>,
    subagent: Option<&Subagent>,
    toggle: &Toggle,
    cx: &App,
) -> AnyElement {
    let calls = subagent.map_or(0, |s| s.calls);
    let line = Line::new(IconName::Bot, call.title())
        .part(call.kind.map(|kind| Part::Chip(kind.to_string().into())))
        .part((calls > 0).then(|| Part::Note(count(calls, "call", "calls").into())));
    let opened = toggle.open.then(|| opened(call, answer, subagent, toggle, cx));
    line::drawn(line, answer, opened, toggle, cx)
}

/// What it was asked, then what it did, under a rule like a thread.
fn opened(call: &Agent, answer: Option<&Answer>, subagent: Option<&Subagent>, toggle: &Toggle, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let prompt = call.prompt.map(|prompt| {
        block::frame(cx).child(block::label("Prompt", cx)).child(block::prose(toggle.key, "prompt", prompt))
    });
    let events = subagent.and_then(|s| s.events.as_deref()).unwrap_or_default();
    let mut work = Vec::new();
    for (event, around) in events {
        // Drawn in the call's row, so opening one of its calls makes this row taller.
        let drawn = blocks_of(&event.body)
            .iter()
            .zip(&around.blocks)
            .filter_map(|(block, drawing)| render_block(toggle.seq, block, drawing, toggle.this, cx));
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
        .children(prompt)
        .children(work)
        .when_some(report, |d, report| d.child(agent_text(toggle.key, report)))
        .into_any_element()
}
