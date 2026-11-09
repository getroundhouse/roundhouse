//! A tool call, and the result it got back, drawn together. They arrive as separate events, often with other calls
//! in between when the agent runs several at once, so each call finds its own result by ID.

use client_core::session_log::Row;
use gpui_kit::base::SelectableText;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, FontWeight, IntoElement, ParentElement, SharedString, Styled, WeakEntity, div, px,
};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{Seq, ToolUseId};
use serde_json::Value;

use super::disclosure;
use super::tail::first_line;
use crate::log_block::{LogBlock, LogText};
use crate::transcript::{Model, Transcript};

/// How many events apart a call and its result can be and still be drawn together. A result follows its call
/// within a few events. One further away is drawn where it is.
const NEAR: usize = 64;
/// Above this, output goes through [`LogBlock`], which can hold anything but has no text selection yet.
const LOG_BLOCK_ABOVE: usize = 16 * 1024;
/// How far a result sits in from its call, so it lines up under the call's name.
const INDENT: f32 = 20.0;

/// A call's result, found in the log, to draw under the call.
pub(super) struct Answer {
    seq: Seq,
    block: usize,
    is_error: bool,
    text: String,
    expanded: bool,
}

/// The result for each call among an event's `blocks`, by the block's position.
pub(super) fn answers(m: &Model, seq: Seq, blocks: &[ContentBlock]) -> Vec<Option<Answer>> {
    let after = usize::try_from(seq).unwrap_or(usize::MAX);
    blocks
        .iter()
        .map(|block| {
            let ContentBlock::ToolUse { id, .. } = block else { return None };
            (after..after.saturating_add(NEAR)).find_map(|ix| result_in(m, ix, id))
        })
        .collect()
}

fn result_in(m: &Model, ix: usize, call: &str) -> Option<Answer> {
    let Row::Event(event) = m.log.row(ix) else { return None };
    let EventBody::AgentUser { blocks, .. } = &event.body else { return None };
    blocks.iter().enumerate().find_map(|(block, content)| {
        let ContentBlock::ToolResult { tool_use_id, is_error, text, .. } = content else { return None };
        (tool_use_id.as_str() == call).then(|| Answer {
            seq: event.seq,
            block,
            is_error: *is_error,
            text: text.clone(),
            expanded: m.expanded.contains(&(event.seq, block)),
        })
    })
}

/// Whether the result in an event at `seq` is drawn with its call, which is a little earlier in the log.
pub(super) fn drawn_with_call(m: &Model, seq: Seq, result: &ToolUseId) -> bool {
    let at = usize::try_from(seq).unwrap_or(usize::MAX).saturating_sub(1);
    (at.saturating_sub(NEAR)..at).rev().any(|ix| {
        let Row::Event(event) = m.log.row(ix) else { return false };
        let EventBody::Assistant { blocks, .. } = &event.body else { return false };
        blocks.iter().any(|block| matches!(block, ContentBlock::ToolUse { id, .. } if id == result.as_str()))
    })
}

/// One line for the call, with its result folded underneath when it's back.
pub(super) fn call(
    name: &str,
    input: &Value,
    answer: Option<&Answer>,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let line = h_flex()
        .h(px(24.))
        .gap_2()
        .text_xs()
        .child(Icon::new(icon(name)).xsmall().text_color(look::faint()))
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).child(SharedString::from(name.to_string())))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .child(summarize_input(input)),
        );
    v_flex()
        .w_full()
        .child(line)
        .when_some(answer, |d, a| {
            d.child(result(a.seq, a.block, a.is_error, &a.text, a.expanded, this, cx).pl(px(INDENT)))
        })
        .into_any_element()
}

/// A result's line, and its output when it's opened.
pub(super) fn result(
    seq: Seq,
    block: usize,
    is_error: bool,
    text: &str,
    expanded: bool,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> Div {
    let label = if is_error { "Error" } else { "Output" };
    let lines = text.lines().count();
    let summary = if lines == 1 { "1 line".to_string() } else { format!("{lines} lines") };
    let id = ElementId::NamedInteger("output".into(), (seq << 8) | block as u64);
    disclosure(
        ElementId::NamedInteger("result".into(), (seq << 8) | block as u64),
        label,
        summary,
        expanded,
        seq,
        block,
        this,
        cx,
    )
    .when(is_error, |d| d.text_color(cx.theme().danger))
    .when(expanded, |d| d.child(output(id, text, is_error, cx)))
}

fn output(id: ElementId, text: &str, is_error: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let color = if is_error { theme.danger } else { theme.muted_foreground };
    let font = theme.mono_font_family.clone();
    let frame = div()
        .ml(px(INDENT))
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .font_family(font.clone())
        .text_size(theme.mono_font_size)
        .text_color(color);
    if text.len() > LOG_BLOCK_ABOVE {
        frame.child(LogBlock { log: LogText::new(text), color, font }).into_any_element()
    } else {
        frame.child(SelectableText::new(id, text.to_string())).into_any_element()
    }
}

/// A small picture of what kind of tool it is.
fn icon(name: &str) -> IconName {
    match name {
        "Bash" => IconName::SquareTerminal,
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => IconName::FileText,
        "Grep" | "Glob" | "WebSearch" | "WebFetch" => IconName::Search,
        _ => IconName::Ellipsis,
    }
}

/// One line saying what a tool is doing. Shows the field that matters for common tools.
pub(crate) fn summarize_input(input: &Value) -> SharedString {
    for key in ["command", "file_path", "path", "pattern", "url", "query", "description", "prompt"] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            return first_line(value).into();
        }
    }
    first_line(&input.to_string()).into()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use client_core::session_log::SessionLog;
    use protocol::event::{ContentBlock, EventBody, SessionEvent};
    use protocol::model::{Seq, ToolUseId};
    use serde_json::json;

    use super::{answers, drawn_with_call, summarize_input};
    use crate::transcript::Model;

    fn event(seq: Seq, body: EventBody) -> SessionEvent {
        SessionEvent { session_id: "s".into(), seq, ts_ms: 0, run: None, body, raw: None }
    }

    fn calls(ids: &[&str]) -> EventBody {
        let blocks = ids
            .iter()
            .map(|id| ContentBlock::ToolUse { id: (*id).to_string(), name: "Bash".into(), input: json!({}) })
            .collect();
        EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks }
    }

    fn result(id: &str) -> EventBody {
        let block = ContentBlock::ToolResult {
            tool_use_id: id.into(),
            is_error: false,
            text: format!("from {id}"),
            image_count: 0,
        };
        EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![block] }
    }

    fn model(events: Vec<SessionEvent>) -> Model {
        let mut log = SessionLog::new(0);
        log.install_page(1, events, 0);
        Model {
            log,
            expanded: HashSet::new(),
            picks: HashMap::<_, BTreeMap<_, _>>::new(),
            focus: 0,
            fetch_scheduled: false,
        }
    }

    #[test]
    fn calls_made_at_once_each_get_their_own_result() {
        let m = model(vec![event(1, calls(&["a", "b"])), event(2, result("b")), event(3, result("a"))]);
        let EventBody::Assistant { blocks, .. } = calls(&["a", "b"]) else { unreachable!() };

        let found: Vec<Option<String>> = answers(&m, 1, &blocks).into_iter().map(|a| a.map(|a| a.text)).collect();

        assert_eq!(found, [Some("from a".to_string()), Some("from b".to_string())]);
        assert!(drawn_with_call(&m, 2, &ToolUseId::from("b")));
        assert!(drawn_with_call(&m, 3, &ToolUseId::from("a")));
    }

    #[test]
    fn a_result_without_its_call_nearby_is_drawn_where_it_is() {
        let m = model(vec![event(1, calls(&["a"])), event(2, result("elsewhere"))]);
        let EventBody::Assistant { blocks, .. } = calls(&["a"]) else { unreachable!() };

        assert!(!drawn_with_call(&m, 2, &ToolUseId::from("elsewhere")));
        assert!(answers(&m, 1, &blocks)[0].is_none(), "a call whose result hasn't come back has nothing under it");
    }

    #[test]
    fn tool_summaries_pick_the_field_that_matters() {
        assert_eq!(summarize_input(&json!({"command": "cargo test\nsecond line", "description": "x"})), "cargo test");
        assert_eq!(summarize_input(&json!({"file_path": "src/a.rs", "old_string": "y"})), "src/a.rs");
        assert_eq!(summarize_input(&json!({"other": 1})), "{\"other\":1}");
    }
}
