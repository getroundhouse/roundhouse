//! What drawing one event needs from the rest of the log and the model, read while they're held, so nothing is held
//! while it's drawn.

use std::collections::BTreeMap;

use client_core::session_log::SessionLog;
use protocol::event::{ContentBlock, EventBody, FileChange, Hunk, SessionEvent};
use protocol::model::Seq;

use crate::attachments::{self, Carried};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::transcript::Model;

mod nearby;

/// How many events apart a call and its result can be and still be drawn together. A result follows its call
/// within a few events. One further away is drawn where it is.
const NEAR: usize = 64;

pub(super) struct Around {
    /// What each of the event's blocks needs, by position.
    pub(super) blocks: Vec<BlockAround>,
    /// For an approval: whether it's still waiting, and what's been chosen on its questions so far.
    pub(super) approval_open: bool,
    pub(super) picks: BTreeMap<String, Vec<String>>,
    /// For a prompt: the files it carries.
    pub(super) carried: Vec<Carried>,
}

/// What drawing one block of an event needs.
pub(super) struct BlockAround {
    /// What the block is, wherever it's drawn.
    pub(super) key: ItemKey,
    pub(super) open: bool,
    /// For a call: its result, to draw under it, once it's back.
    pub(super) answer: Option<Answer>,
    /// For a result: whether it's drawn with its call instead.
    pub(super) with_call: bool,
}

/// A call's result, found in the log, to draw under the call.
pub(super) struct Answer {
    pub(super) is_error: bool,
    pub(super) text: String,
    /// What it changed in a file, when its agent said.
    pub(super) change: Option<FileChange>,
}

impl Answer {
    /// What it says went wrong, if it failed.
    pub(super) fn failure(&self) -> Option<&str> {
        self.is_error.then_some(self.text.as_str())
    }

    /// What it sent back, if it didn't fail.
    pub(super) fn success(&self) -> Option<&str> {
        (!self.is_error).then_some(self.text.as_str())
    }

    /// The parts of a file it changed, when its agent said which they were.
    pub(super) fn hunks(&self) -> Option<&[Hunk]> {
        let Some(FileChange::Changed { hunks }) = &self.change else { return None };
        (!hunks.is_empty()).then_some(hunks.as_slice())
    }
}

impl Around {
    pub(super) fn read(log: &SessionLog, m: &Model, seq: Seq, body: &EventBody) -> Self {
        let blocks = blocks_of(body)
            .iter()
            .enumerate()
            .map(|(position, block)| BlockAround::read(log, m, seq, block, block_key(seq, body, position)))
            .collect();
        let mut around = Around { blocks, approval_open: false, picks: BTreeMap::new(), carried: Vec::new() };
        if let EventBody::UserPrompt { attachments, .. } = body {
            around.carried = attachments::carried(m, attachments);
        }
        if let EventBody::ApprovalRequested { approval_id, .. } = body {
            around.approval_open = log
                .open_approvals()
                .iter()
                .any(|e| matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id));
            around.picks = m.picks.get(approval_id).cloned().unwrap_or_default();
        }
        around
    }
}

impl BlockAround {
    fn read(log: &SessionLog, m: &Model, seq: Seq, block: &ContentBlock, key: ItemKey) -> Self {
        let open = m.folds.is_open(&key, Fold::of(block));
        let answer = if let ContentBlock::ToolUse { id, .. } = block {
            nearby::events_after(log, seq, NEAR).find_map(|event| answer_in(event, id))
        } else {
            None
        };
        // Drawn with its call when the call is close enough before it to have found it.
        let with_call = matches!(block, ContentBlock::ToolResult { tool_use_id, .. }
            if nearby::call_before(log, seq, tool_use_id.as_str(), NEAR).is_some());
        Self { key, open, answer, with_call }
    }
}

/// The result of the call `id`, if it's in `event`.
fn answer_in(event: &SessionEvent, id: &str) -> Option<Answer> {
    let EventBody::AgentUser { blocks, .. } = &event.body else { return None };
    blocks.iter().find_map(|block| {
        let ContentBlock::ToolResult { tool_use_id, is_error, text, change, .. } = block else { return None };
        (tool_use_id.as_str() == id).then(|| Answer { is_error: *is_error, text: text.clone(), change: change.clone() })
    })
}

/// The blocks an event holds, if it's one that holds any.
pub(super) fn blocks_of(body: &EventBody) -> &[ContentBlock] {
    if let EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } = body { blocks } else { &[] }
}

/// What the block at `position` in an event is, wherever it's drawn. One the agent wrote keeps the key it had
/// while it streamed.
fn block_key(seq: Seq, body: &EventBody, position: usize) -> ItemKey {
    if let EventBody::Assistant { message_id, first_index, .. } = body {
        return ItemKey::recorded_block(seq, position, message_id.as_ref(), *first_index);
    }
    ItemKey::Event(seq, position)
}

#[cfg(test)]
mod tests {
    use protocol::event::{ContentBlock, EventBody};

    use super::{Around, block_key};
    use crate::fixtures::{calls, event, log, result};
    use crate::item_key::ItemKey;
    use crate::transcript::Model;

    #[test]
    fn a_recorded_block_is_known_by_its_place_in_its_message() {
        let finished = |first_index| EventBody::Assistant {
            message_id: Some("m".into()),
            parent_tool_use_id: None,
            model: None,
            blocks: vec![ContentBlock::Text { text: "a".into() }],
            first_index,
        };

        assert_eq!(block_key(7, &finished(Some(2)), 0), ItemKey::Block("m".into(), 2));
        assert_eq!(block_key(7, &finished(None), 0), ItemKey::Event(7, 0), "nothing streamed");
    }

    #[test]
    fn calls_made_at_once_each_get_their_own_result() {
        let log = log([event(1, calls(&["a", "b"])), event(2, result("b")), event(3, result("a"))]);
        let m = Model::default();

        let around = Around::read(&log, &m, 1, &calls(&["a", "b"]));

        let found: Vec<Option<String>> = around.blocks.into_iter().map(|b| b.answer.map(|a| a.text)).collect();
        assert_eq!(found, [Some("from a".to_string()), Some("from b".to_string())]);
        assert!(Around::read(&log, &m, 2, &result("b")).blocks[0].with_call);
        assert!(Around::read(&log, &m, 3, &result("a")).blocks[0].with_call);
    }

    #[test]
    fn a_result_without_its_call_nearby_is_drawn_where_it_is() {
        let log = log([event(1, calls(&["a"])), event(2, result("elsewhere"))]);
        let m = Model::default();

        assert!(!Around::read(&log, &m, 2, &result("elsewhere")).blocks[0].with_call);
        let call = Around::read(&log, &m, 1, &calls(&["a"]));
        assert!(call.blocks[0].answer.is_none(), "a call whose result hasn't come back has nothing under it");
    }
}
