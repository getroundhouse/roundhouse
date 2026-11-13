//! The subagent tab: one subagent's work, which its log holds alone, between the blocks its call shows in the chat.
//! What it was asked comes above its first row, and its report, once it's done, in its last. Its last words, which
//! its report says again, aren't drawn twice.

use std::sync::Arc;
use std::time::Duration;

use client_core::session_log::{Row, SessionLog};
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Subagent, SubagentStatus};

use super::around::blocks_of;
use super::tools::agent;
use super::turn_times;
use super::{Shared, from_blocks, row};
use crate::calls::count;
use crate::find::{self, FindLook, Part};
use crate::item_key::ItemKey;
use crate::model_names::model_name;
use crate::texts::{Of, TextAt, Written};
use crate::transcript::Model;

/// Where a subagent stands, for the top of its tab, such as `Finished · Haiku 5.5 · 3 calls · 18.2k context · 12s`.
/// Its tokens are called its context, since they're what its conversation holds, where its calls are a total.
pub fn subagent_standing(subagent: &Subagent) -> String {
    // One this build doesn't know isn't given a word, as the call's line gives it none.
    let status = match subagent.status {
        SubagentStatus::Working => Some("Working"),
        SubagentStatus::Finished => Some("Finished"),
        SubagentStatus::Failed => Some("Failed"),
        SubagentStatus::Stopped => Some("Stopped"),
        SubagentStatus::Unknown => None,
    };
    let calls = count(usize::try_from(subagent.calls).unwrap_or(usize::MAX), "call", "calls");
    let took = subagent.ended_at_ms.map(|ended| {
        turn_times::took(Duration::from_millis(u64::try_from(ended - subagent.started_at_ms).unwrap_or(0)))
    });
    let model = subagent.model.as_ref().map(model_name);
    let context = subagent.context_tokens.map(|tokens| format!("{} context", look::tokens(tokens)));
    [status.map(str::to_string), model, Some(calls), context, took]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The subagent the tab shows, once its record has come.
fn shown(m: &Model) -> Option<&Subagent> {
    m.subagents.get(m.alone.as_ref()?).map(Arc::as_ref)
}

/// What the subagent was asked, above the tab's first row, drawn from the kit's blocks, and selected with the rest.
pub(super) fn head(m: &Model, shared: &Shared, cx: &App) -> Option<AnyElement> {
    let subagent = shown(m)?;
    let prompt = subagent.prompt.as_deref()?;
    let key = ItemKey::Call(subagent.call.clone());
    // Nothing's looked for in the subagent tab, so the area that scrolls it has no match to bring into view.
    let find = FindLook::default();
    let element = find::view_id(&(key.clone(), Part::Asked));
    let text =
        |area| from_blocks(&Written::unfound(TextAt::first_of(Of::Head), element, prompt).in_area(area), None, shared);
    let asked = agent::prompt_block(&key, text, &find, &shared.selection, cx);
    Some(row().child(asked).into_any_element())
}

/// Its report, once it's done, in the tab's last row, drawn from the kit's blocks, and selected with the rest. That's
/// the tail's, which no event is in, so an image it points to doesn't load, as one in what the agent is still writing
/// doesn't.
pub(super) fn foot(m: &Model, shared: &Shared, cx: &App) -> Option<AnyElement> {
    let report = report(m)?;
    let key = ItemKey::Call(shown(m)?.call.clone());
    let element = find::view_id(&(key.clone(), Part::Report));
    let report = from_blocks(&Written::unfound(TextAt::first_of(Of::Foot), element, report), None, shared);
    Some(row().child(agent::report_block(&key, report, cx)).into_any_element())
}

fn report(m: &Model) -> Option<&str> {
    shown(m).filter(|subagent| subagent.status != SubagentStatus::Working)?.report.as_deref()
}

/// The rows whose events say only what the report says, so they're drawn empty: those of the subagent's last message,
/// when its words are the report's. The CLI sends a message as an event a block, so that's the newest events that share
/// a message's ID, and one of them that says more than words, such as its thinking, is drawn as it is.
pub(crate) fn rows_repeating_report(log: &SessionLog, m: &Model) -> Vec<usize> {
    let Some(report) = report(m) else { return Vec::new() };
    let message = last_message(log);
    let words = message.iter().flat_map(|(_, event)| blocks_of(&event.body)).filter_map(|block| match block {
        ContentBlock::Text { text, .. } => Some(text.split_whitespace()),
        ContentBlock::Thinking { .. }
        | ContentBlock::ToolUse { .. }
        | ContentBlock::ToolResult { .. }
        | ContentBlock::Image { .. }
        | ContentBlock::Unknown => None,
    });
    if message.is_empty() || !words.flatten().eq(report.split_whitespace()) {
        return Vec::new();
    }
    let only_words = |event: &SessionEvent| {
        let blocks = blocks_of(&event.body);
        !blocks.is_empty() && blocks.iter().all(|block| matches!(block, ContentBlock::Text { .. }))
    };
    message.into_iter().filter(|(_, event)| only_words(event)).map(|(row, _)| row).collect()
}

/// Whether the event in `row` says only what the report says, so the report's block is all that's drawn of it.
pub(super) fn repeats_report(log: &SessionLog, m: &Model, row: usize) -> bool {
    report(m).is_some() && rows_repeating_report(log, m).contains(&row)
}

/// The rows of the newest message the subagent wrote, oldest first, and their events: the newest `Assistant` events
/// that share its ID, past anything else after them. One with no ID is a message alone.
fn last_message(log: &SessionLog) -> Vec<(usize, &SessionEvent)> {
    let mut message: Vec<(usize, &SessionEvent)> = Vec::new();
    for row in (0..log.row_count()).rev() {
        let Row::Event(event) = log.row(row) else { break };
        let EventBody::Assistant { message_id, .. } = &event.body else {
            if message.is_empty() {
                continue;
            }
            break;
        };
        let same = |(_, held): &(usize, &SessionEvent)| matches!(&held.body, EventBody::Assistant { message_id: Some(id), .. } if Some(id) == message_id.as_ref());
        if message.first().is_some_and(|held| !same(held)) {
            break;
        }
        message.insert(0, (row, event));
        if message_id.is_none() {
            break;
        }
    }
    message
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use protocol::event::{ContentBlock, EventBody, SessionEvent};
    use protocol::model::{ModelName, Seq, Subagent, SubagentStatus};

    use super::{rows_repeating_report, subagent_standing};
    use crate::fixtures::{calls, event, log, record, within};
    use crate::transcript::Model;

    fn subagent(status: SubagentStatus, report: Option<&str>) -> Subagent {
        Subagent {
            kind: Some("Explore".into()),
            description: Some("Find checks".into()),
            prompt: Some("Look".into()),
            started_at_ms: 1_000,
            ended_at_ms: (status != SubagentStatus::Working).then_some(13_000),
            calls: 3,
            report: report.map(str::to_string),
            ..record("agent", status)
        }
    }

    /// The tab showing `subagent`.
    fn tab(subagent: Subagent) -> Model {
        let mut m = Model { alone: Some(subagent.call.clone()), ..Model::default() };
        m.subagents.insert(subagent.call.clone(), Arc::new(subagent));
        m
    }

    fn assistant(message_id: Option<&str>, blocks: Vec<ContentBlock>) -> EventBody {
        let message_id = message_id.map(Into::into);
        within(
            "agent",
            EventBody::Assistant { message_id, parent_tool_use_id: None, model: None, blocks, first_index: None },
        )
    }

    fn text(text: &str) -> ContentBlock {
        ContentBlock::Text { text: text.to_string(), images: Vec::new() }
    }

    /// One event saying `texts`, a block each.
    fn said(seq: Seq, texts: &[&str]) -> SessionEvent {
        event(seq, assistant(None, texts.iter().map(|t| text(t)).collect()))
    }

    /// The message `id` from `first` on, as the CLI sends one: an event a block.
    fn message(first: Seq, id: &str, blocks: Vec<ContentBlock>) -> Vec<SessionEvent> {
        (first..).zip(blocks).map(|(seq, block)| event(seq, assistant(Some(id), vec![block]))).collect()
    }

    #[test]
    fn the_tab_leaves_out_last_words_only_when_its_report_says_all_of_them() {
        let m = tab(subagent(SubagentStatus::Finished, Some("Two places.\n\nBoth compare directly.")));
        let repeated = |events: Vec<SessionEvent>| rows_repeating_report(&log(events), &m);
        let thinking = ContentBlock::Thinking { text: "Where?".into() };
        let mut sent = vec![event(1, within("agent", calls(&["a"])))];
        sent.extend(message(2, "m", vec![thinking, text("Two places."), text("Both compare directly.")]));

        assert_eq!(repeated(sent), [2, 3], "as the CLI sends them, its words but not its thinking");
        assert_eq!(repeated(vec![said(1, &["Two places.\n\nBoth compare directly."])]), [0]);
        assert_eq!(repeated(vec![said(1, &["Two places.", "Both compare directly."])]), [0], "two blocks in one event");
        assert!(repeated(message(1, "m", vec![text("Two places.")])).is_empty(), "only part of it");
        let mut earlier = message(1, "m", vec![text("Two places."), text("Both compare directly.")]);
        earlier.extend(message(3, "n", vec![text("Done.")]));
        assert!(repeated(earlier).is_empty(), "words before its last message");
        assert!(repeated(vec![event(1, within("agent", calls(&["a"])))]).is_empty(), "a call");
        let working = tab(subagent(SubagentStatus::Working, None));
        assert!(rows_repeating_report(&log(vec![said(1, &["Two places."])]), &working).is_empty(), "no report yet");
    }

    #[test]
    fn where_a_subagent_stands_says_how_it_ended_its_model_its_calls_its_context_and_how_long_it_took() {
        let finished = subagent(SubagentStatus::Finished, Some("Done."));
        let working = Subagent { calls: 1, ..subagent(SubagentStatus::Working, None) };
        let on_haiku = Subagent { model: Some(ModelName::named("claude-haiku-5-5")), ..finished.clone() };
        let counted = Subagent { context_tokens: Some(18_240), ..on_haiku.clone() };
        let counting = Subagent { context_tokens: Some(950), ..working.clone() };

        assert_eq!(subagent_standing(&counted), "Finished · Haiku 5.5 · 3 calls · 18.2k context · 12s");
        assert_eq!(subagent_standing(&counting), "Working · 1 call · 950 context");
        assert_eq!(subagent_standing(&on_haiku), "Finished · Haiku 5.5 · 3 calls · 12s", "before the agent counts");
        assert_eq!(subagent_standing(&finished), "Finished · 3 calls · 12s", "before it's replied");
        assert_eq!(subagent_standing(&working), "Working · 1 call", "how long it took is for once it's done");
        assert_eq!(subagent_standing(&subagent(SubagentStatus::Stopped, None)), "Stopped · 3 calls · 12s");
        assert_eq!(subagent_standing(&subagent(SubagentStatus::Unknown, None)), "3 calls · 12s", "one it doesn't know");
    }
}
