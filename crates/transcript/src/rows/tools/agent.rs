//! A call that handed work to a subagent: what it's for, what kind of agent, and how many calls it's made. Opened,
//! what it was asked, then everything it did, under a rule, the way the transcript draws the agent's own work. Each of
//! the subagent's events is drawn in a row of its own after the call, as [`crate::placement`] says, so the rule runs
//! down through those rows, and the last of them ends it.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement, Styled};

use super::line::{Line, Part};
use super::{Answer, Subagent, Toggle, block, line};
use crate::calls::agent::{self, Agent};
use crate::calls::count;
use crate::linked_images::LinkedImages;
use crate::rows::around::Under;
use crate::rows::{agent_text, empty};

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

/// What it was asked, at the top of the rule its work is drawn under. The work follows in rows of its own, the last
/// of which ends the rule. With no work yet, this ends it, with the call's result when that's the subagent's report.
fn opened(call: &Agent, answer: Option<&Answer>, subagent: Option<&Subagent>, toggle: &Toggle, cx: &App) -> AnyElement {
    let follows = subagent.is_some_and(|s| s.follows);
    let prompt = call.prompt.map(|prompt| {
        block::frame(cx).child(block::label("Prompt", cx)).child(block::prose(toggle.key, "prompt", prompt))
    });
    // A subagent run in the foreground reports in its call's result.
    let report = answer.and_then(Answer::success).filter(|text| !follows && !agent::only_launched(text));
    rule(cx)
        .mt_1()
        .when(!follows, Styled::mb_2)
        .children(prompt)
        .when_some(report, |d, report| {
            d.child(agent_text(toggle.key, report).image_source(toggle.linked.resolver(toggle.seq)))
        })
        .into_any_element()
}

/// One of a subagent's events, in its own row under the rule of each call it's drawn under, outermost first. Between
/// rows the rule runs on unbroken. The last row a call's work reaches ends its rule, with the report from its result
/// if the subagent wrote none, then the space its call's row would have had below it.
pub(in crate::rows) fn work(blocks: Vec<AnyElement>, under: &[Under], linked: &LinkedImages, cx: &App) -> AnyElement {
    if blocks.is_empty() && !under.iter().any(|call| call.ends) {
        return empty();
    }
    let mut blocks = Some(blocks);
    let mut nested: Option<AnyElement> = None;
    for call in under.iter().rev() {
        let innermost = blocks.is_some();
        let report = call
            .report
            .as_deref()
            .map(|text| agent_text(&call.key, text).image_source(linked.resolver(call.seq)).into_any_element());
        let items: Vec<AnyElement> =
            blocks.take().unwrap_or_default().into_iter().chain(nested.take()).chain(report).collect();
        // The space between one thing drawn under the rule and the next, which is the one before it in another row.
        let rule = (!items.is_empty()).then(|| rule(cx).when(innermost, Styled::pt_2).children(items));
        nested = Some(v_flex().children(rule).when(call.ends, Styled::pb_2).into_any_element());
    }
    let ends = under.first().is_some_and(|call| call.ends);
    v_flex().children(nested).when(ends, Styled::pb_2).into_any_element()
}

/// The rule a subagent's work is drawn under. It's on the column's left edge, like everything else, and the work sits
/// in from it to show it's nested.
fn rule(cx: &App) -> Div {
    v_flex().pl_3().gap_2().border_l_1().border_color(cx.theme().border)
}
