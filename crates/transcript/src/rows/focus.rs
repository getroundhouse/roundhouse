//! The subagent tab: one subagent's work, drawn from the chat's log by leaving out everything else. A sketch for
//! #417, on a branch that's never merged, which draws each option [`crate::sketch`] names.

use std::time::Duration;

use client_core::session_log::{PromptState, Row, SessionLog};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, FontWeight, IntoElement, ParentElement, Styled, WeakEntity, div, px};
use look::{Signal, Work};
use protocol::event::{ContentBlock, EventBody, SessionEvent, SubagentOutcome};
use protocol::model::{Seq, ToolUseId};

use super::around::{Around, Subagent, blocks_of};
use super::prompt::{Progress, prompt_bubble};
use super::tools::block;
use super::{agent_text, empty, row, turn_times};
use crate::calls::agent::{Agent, hands_off};
use crate::calls::count;
use crate::item_key::ItemKey;
use crate::sketch::{Sketch, sketch};
use crate::transcript::{Model, Transcript};

/// Where a subagent has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusStatus {
    Working,
    Finished,
    Failed,
    Stopped,
}

/// What the subagent tab shows of a subagent, for its title, its mark and the top of its page.
#[derive(Clone, Debug)]
pub struct FocusSummary {
    pub title: String,
    pub kind: Option<String>,
    pub prompt: Option<String>,
    pub status: FocusStatus,
    pub calls: usize,
    /// How long it took, once it ended.
    pub took: Option<String>,
    pub started_at_ms: Option<i64>,
    pub report: Option<String>,
}

impl FocusSummary {
    /// Where it stands, in a few words, such as `Finished · 4 calls · 52s`.
    pub fn standing(&self) -> String {
        let status = match self.status {
            FocusStatus::Working => "Working",
            FocusStatus::Finished => "Finished",
            FocusStatus::Failed => "Failed",
            FocusStatus::Stopped => "Stopped",
        };
        let calls = count(self.calls, "call", "calls");
        [Some(status.to_string()), Some(calls), self.took.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }
}

/// What the call `call` started, in `log`, if the call is there.
pub(crate) fn focus_summary(log: &SessionLog, call: &ToolUseId) -> Option<FocusSummary> {
    let (seq, ts_ms, input) = (0..log.row_count()).find_map(|ix| {
        let Row::Event(event) = log.row(ix) else { return None };
        let EventBody::Assistant { blocks, parent_tool_use_id: None, .. } = &event.body else { return None };
        blocks.iter().find_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } if id == call.as_str() && hands_off(name) => {
                Some((event.seq, event.ts_ms, input.clone()))
            }
            ContentBlock::ToolUse { .. }
            | ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => None,
        })
    })?;
    let agent = Agent::read(&input);
    let subagent = Subagent::read(log, &Model::default(), seq, call.as_str(), false);
    let status = match subagent.ended.as_ref().map(|ended| ended.outcome) {
        None => FocusStatus::Working,
        Some(SubagentOutcome::Failed) => FocusStatus::Failed,
        Some(SubagentOutcome::Stopped) => FocusStatus::Stopped,
        Some(SubagentOutcome::Finished | SubagentOutcome::Unknown) => FocusStatus::Finished,
    };
    let started_at_ms = subagent.started_at_ms.or(Some(ts_ms));
    let took = subagent.ended.as_ref().zip(started_at_ms).map(|(ended, started)| {
        turn_times::took(Duration::from_millis(u64::try_from(ended.at_ms - started).unwrap_or(0)))
    });
    Some(FocusSummary {
        title: agent.title().to_string(),
        kind: agent.kind.map(str::to_string),
        prompt: agent.prompt.map(str::to_string),
        status,
        calls: subagent.calls,
        took,
        started_at_ms,
        report: subagent.ended.and_then(|ended| ended.report),
    })
}

/// What one row of the tab shows.
pub(super) enum Part {
    /// The call that started it, which the tab draws as what it was asked.
    Asked(Seq, FocusSummary),
    /// One of its own events, drawn as the chat draws the agent's.
    Work(SessionEvent, Around),
    /// The agent's word that it ended.
    Ended(Seq, FocusSummary),
    /// The end of the list, while it works.
    Working(FocusSummary),
    Nothing,
}

impl Part {
    /// Whether it's drawn under the rule that runs down the thread, in the option that has one.
    pub(super) fn in_thread(&self) -> bool {
        matches!(self, Part::Work(..) | Part::Ended(..) | Part::Working(..))
    }
}

pub(super) fn part(log: &SessionLog, m: &Model, call: &ToolUseId, ix: usize) -> Part {
    let summary = || focus_summary(log, call);
    if ix >= log.row_count() {
        return match summary() {
            Some(summary) if summary.status == FocusStatus::Working => Part::Working(summary),
            Some(_) | None => Part::Nothing,
        };
    }
    let Row::Event(event) = log.row(ix) else { return Part::Nothing };
    match &event.body {
        EventBody::Assistant { blocks, parent_tool_use_id: None, .. }
            if blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { id, .. } if id == call.as_str())) =>
        {
            summary().map_or(Part::Nothing, |summary| Part::Asked(event.seq, summary))
        }
        EventBody::SubagentEnded { call: ended, .. } if ended == call => {
            summary().map_or(Part::Nothing, |summary| Part::Ended(event.seq, summary))
        }
        EventBody::Assistant { parent_tool_use_id: Some(parent), .. }
        | EventBody::AgentUser { parent_tool_use_id: Some(parent), .. }
            if parent == call =>
        {
            // On a page, the report has a block of its own at the end, so the words it's made of aren't drawn twice.
            let report = summary().and_then(|summary| summary.report);
            if sketch() == Sketch::Page && report.as_deref().is_some_and(|report| only_says(event, report)) {
                return Part::Nothing;
            }
            Part::Work(event.clone(), Around::read(log, m, event.seq, &event.body))
        }
        EventBody::UserPrompt { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => Part::Nothing,
    }
}

/// Whether all `event` says is `report`.
fn only_says(event: &SessionEvent, report: &str) -> bool {
    matches!(blocks_of(&event.body), [ContentBlock::Text { text }] if text.trim() == report.trim())
}

/// What a part looks like in the option being drawn, except a subagent's own event, which the chat's rows draw.
pub(super) fn render(part: Part, this: &WeakEntity<Transcript>, cx: &App) -> AnyElement {
    match (part, sketch()) {
        (Part::Asked(seq, summary), Sketch::Chat) => {
            let prompt = summary.prompt.unwrap_or_default();
            let progress = Progress::Recorded(seq, PromptState::Taken);
            prompt_bubble(&ItemKey::Event(seq, 0), &prompt, None, &[], progress, this, cx)
        }
        (Part::Asked(seq, summary), Sketch::Page) => row()
            .child(block::frame(cx).child(block::label("Prompt", cx)).child(block::prose(
                &ItemKey::Event(seq, 0),
                "asked",
                &summary.prompt.unwrap_or_default(),
            )))
            .into_any_element(),
        (Part::Asked(seq, summary), Sketch::Thread) => thread_head(seq, &summary, cx),
        (Part::Ended(seq, summary), Sketch::Page) => report_block(seq, &summary, cx),
        (Part::Ended(_, summary), Sketch::Chat | Sketch::Thread) => {
            row().child(div().text_xs().text_color(look::faint()).child(summary.standing())).into_any_element()
        }
        (Part::Working(summary), _) => working(&summary, cx),
        (Part::Work(..) | Part::Nothing, _) => empty(),
    }
}

/// The rule down the thread's left, with what's in it sitting in from it.
pub(super) fn in_thread(content: AnyElement, cx: &App) -> AnyElement {
    div().pl_4().ml(px(7.)).border_l_1().border_color(cx.theme().border).child(content).into_any_element()
}

/// The thread's top: the call's own line, larger, then what it was asked, where the rule starts.
fn thread_head(seq: Seq, summary: &FocusSummary, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let chip = summary.kind.clone().map(|kind| {
        div()
            .px_2()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.input)
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(kind)
    });
    let head = h_flex()
        .gap_2()
        .pt_2()
        .child(Icon::new(IconName::Bot).size(px(16.)).text_color(look::faint()))
        .child(div().text_base().font_weight(FontWeight::MEDIUM).child(summary.title.clone()))
        .children(chip)
        .child(div().text_sm().text_color(look::faint()).child(count(summary.calls, "call", "calls")));
    let prompt = summary.prompt.clone().unwrap_or_default();
    let asked = div().text_color(theme.muted_foreground).child(agent_text(&ItemKey::Event(seq, 0), &prompt, None));
    v_flex().w_full().gap_3().child(head).child(in_thread(row().child(asked).into_any_element(), cx)).into_any_element()
}

/// The report at the end of a page, on a block of its own, as the prompt is at its top.
fn report_block(seq: Seq, summary: &FocusSummary, cx: &App) -> AnyElement {
    let Some(report) = &summary.report else {
        return row().child(div().text_xs().text_color(look::faint()).child(summary.standing())).into_any_element();
    };
    row()
        .child(block::frame(cx).child(block::label("Report", cx)).child(div().px_3().py_2().child(agent_text(
            &ItemKey::Event(seq, 1),
            report,
            None,
        ))))
        .into_any_element()
}

/// The mark with its light sweeping, as the chat's working line has it, and how many calls it's made so far.
fn working(summary: &FocusSummary, cx: &App) -> AnyElement {
    line(cx)
        .child(look::mark(Signal::Working, Work::NoPr, look::Size::Inline, cx))
        // A page's band says where it stands already.
        .when(sketch() != Sketch::Page, |d| d.child(format!("Working · {}", count(summary.calls, "call", "calls"))))
        .into_any_element()
}

fn line(cx: &App) -> Div {
    div().flex().items_center().gap_2().h(px(super::line::HEIGHT)).text_color(cx.theme().muted_foreground)
}
