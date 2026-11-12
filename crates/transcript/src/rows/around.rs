//! What drawing one event needs from the rest of the log and the model, read while they're held, so nothing is held
//! while it's drawn.

use client_core::Approval;
use client_core::session_log::{PromptAt, PromptState, SessionLog};
use gpui_kit::SharedString;
use protocol::event::{ContentBlock, EventBody, Question, SessionEvent};
use protocol::model::{PermissionMode, Seq, ToolUseId};

use crate::attachments::{self, Carried};
use crate::calls::agent;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::transcript::{IdleCache, Model};

mod nearby;
mod reply;
mod settled;
mod subagent;

pub(super) use reply::{Answer, Reply};
pub(super) use settled::{AnswerTo, Settled};
pub(super) use subagent::Subagent;
pub(crate) use subagent::{REACH, SubagentCalls, drawn_in_its_call, its_call};

/// How many events apart a call and its result can be and still be drawn together. A result follows its call
/// within a few events. One further away is drawn where it is.
pub(crate) const NEAR: usize = 64;

pub(super) struct Around {
    /// What each of the event's blocks needs, by position.
    pub(super) blocks: Vec<BlockAround>,
    /// For an approval: whether it's still waiting.
    pub(super) approval_open: bool,
    /// For a question that's no longer waiting, which is drawn as a call's line.
    pub(super) asked: Option<Asked>,
    /// For the record that closes an approval: whether it closes a question, whose line already says what became of
    /// it.
    pub(super) closes_a_question: bool,
    /// For a prompt: the files it carries.
    pub(super) carried: Vec<Carried>,
    /// For a prompt: whether the agent has it.
    pub(super) prompt: PromptState,
    /// For a turn's end: what the agent the chat runs is called, which its card names, if the hub has said.
    pub(super) agent: Option<SharedString>,
    /// For the newest turn's end: what the cache holds of the conversation while it can expire, which its line and
    /// card say. An older turn's say nothing of it, since only the newest can still be warm.
    pub(super) cache: Option<IdleCache>,
    /// For a change of mode: the mode before it, when the log says.
    pub(super) mode_before: Option<PermissionMode>,
    /// For a compaction: whether it's open to its summary.
    pub(super) open: bool,
}

/// What drawing a question that's no longer waiting needs.
pub(super) struct Asked {
    /// The questions it asked, in order.
    pub(super) questions: Vec<Question>,
    /// What it is, as its event's only item.
    pub(super) key: ItemKey,
    pub(super) open: bool,
    pub(super) settled: Settled,
}

/// What drawing one block of an event needs.
pub(super) struct BlockAround {
    /// What the block is, wherever it's drawn.
    pub(super) key: ItemKey,
    /// The kind of fold it is, and whether that's open.
    pub(super) fold: Fold,
    pub(super) open: bool,
    /// For a command: whether it's shown whole in its opened call, rather than cut off at the end of its line.
    pub(super) whole: bool,
    /// For a call: what's come back for it. For a result drawn on its own: itself.
    pub(super) reply: Reply,
    /// For a call that handed work to a subagent: what the subagent has done.
    pub(super) subagent: Option<Subagent>,
    /// For a result: whether it's drawn with its call instead.
    pub(super) with_call: bool,
}

/// Which blocks are read as open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opening {
    /// As they're drawn: as the person left them, or as their kind starts.
    AsChosen,
    /// Every one, as finding reads them, since it opens what a match is in.
    All,
}

impl Around {
    pub(super) fn read(log: &SessionLog, m: &Model, seq: Seq, body: &EventBody) -> Self {
        Self::read_as(log, m, seq, body, Opening::AsChosen)
    }

    pub(super) fn read_as(log: &SessionLog, m: &Model, seq: Seq, body: &EventBody, opening: Opening) -> Self {
        let blocks = blocks_of(body)
            .iter()
            .enumerate()
            .map(|(position, block)| BlockAround::read(log, m, seq, block, block_key(seq, body, position), opening))
            .collect();
        let mut around = Around {
            blocks,
            approval_open: false,
            asked: None,
            closes_a_question: false,
            carried: Vec::new(),
            prompt: PromptState::Taken,
            agent: m.agent.clone(),
            cache: (seq == m.at_work.reply_seq).then(|| m.at_work.cache.clone()).flatten(),
            mode_before: None,
            open: false,
        };
        if let EventBody::AgentMode { .. } = body {
            around.mode_before = nearby::mode_before(log, seq);
        }
        if matches!(body, EventBody::AgentCompacted { .. }) {
            around.open = m.folds.is_open(&ItemKey::Event(seq, 0), Fold::Output);
        }
        if let EventBody::UserPrompt { attachments, .. } = body {
            around.carried = attachments::carried(m, attachments);
            around.prompt = log.prompt_state(seq);
        }
        if let EventBody::ApprovalRequested { approval_id, .. } = body {
            around.approval_open = log
                .open_approvals()
                .iter()
                .any(|e| matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id));
            let questions = Approval::of(body).and_then(|approval| approval.questions().map(<[Question]>::to_vec));
            if let Some(questions) = questions.filter(|_| !around.approval_open) {
                let key = ItemKey::Event(seq, 0);
                let open = m.folds.is_open(&key, Fold::Output);
                around.asked = Some(Asked { questions, key, open, settled: Settled::read(log, seq, approval_id) });
            }
        }
        if let EventBody::ApprovalResolved { approval_id, .. } = body {
            around.closes_a_question = nearby::events_before(log, seq, NEAR).any(|event| {
                matches!(&event.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id)
                    && asks_questions(&event.body)
            });
        }
        around
    }

    /// Every image the event draws now, for its row to ask the hub for: a prompt's, and those of each call or result
    /// that's open, including what a call gathered from its subagent, or the calls it shows from its subagent's record.
    /// A closed call's images wait until it's opened.
    pub(super) fn images(&self) -> Vec<&Carried> {
        let mut images: Vec<&Carried> = self.carried.iter().collect();
        for block in self.blocks.iter().filter(|block| block.open) {
            images.extend(block.reply.result().into_iter().flat_map(Answer::images));
            let gathered = block.subagent.iter().flat_map(|subagent| subagent.events.iter().flatten());
            images.extend(gathered.flat_map(|(_, around)| around.images()));
            let lately = block.subagent.iter().flat_map(|subagent| &subagent.lately).filter(|(_, around)| around.open);
            images.extend(lately.flat_map(|(_, around)| around.reply.result().into_iter().flat_map(Answer::images)));
        }
        images
    }
}

impl BlockAround {
    pub(super) fn read(
        log: &SessionLog,
        m: &Model,
        seq: Seq,
        block: &ContentBlock,
        key: ItemKey,
        opening: Opening,
    ) -> Self {
        let fold = Fold::of(block);
        let all = opening == Opening::All;
        let open = all || m.folds.is_open(&key, fold);
        let whole = matches!(block, ContentBlock::ToolUse { .. }) && (all || m.folds.is_open(&key, Fold::Command));
        // Drawn with its call when the call is close enough before it to have found it.
        let with_call = matches!(block, ContentBlock::ToolResult { tool_use_id, .. }
            if nearby::call_before(log, seq, tool_use_id, NEAR).is_some());
        let (reply, subagent) = match block {
            ContentBlock::ToolUse { id, name, .. } => {
                let subagent = agent::hands_off(name).then(|| Subagent::read(log, m, seq, id, open, opening));
                (Reply::to(log, m, seq, id), subagent)
            }
            ContentBlock::ToolResult { .. } if !with_call => (Reply::of(block, m), None),
            ContentBlock::ToolResult { .. }
            | ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => (Reply::Nothing, None),
        };
        Self { key, fold, open, whole, reply, subagent, with_call }
    }
}

/// What the row of `event` draws of the log: the event, or the prompt it settled, which is drawn where the agent took
/// it, or nothing for a prompt drawn in another row or at the end, as [`PromptAt`] says.
pub(crate) fn drawn_in_row<'a>(log: &'a SessionLog, event: &'a SessionEvent) -> Option<&'a SessionEvent> {
    if let Some(prompt) = log.prompt_settled_in(event) {
        return Some(prompt);
    }
    let elsewhere = matches!(event.body, EventBody::UserPrompt { .. }) && log.prompt_at(event.seq) != PromptAt::Own;
    (!elsewhere).then_some(event)
}

/// Whether the result `tool_use_id`, in the event at `seq`, is drawn with its call, which is close enough before it to
/// have found it, as [`BlockAround`] says.
pub(crate) fn drawn_with_its_call(log: &SessionLog, seq: Seq, tool_use_id: &ToolUseId) -> Option<Seq> {
    nearby::call_before(log, seq, tool_use_id, NEAR).map(|(call, _)| call)
}

/// Whether `body` is a request for approval that asks the person questions, which is drawn as a call's line once it's
/// no longer waiting, rather than as a card.
pub(super) fn asks_questions(body: &EventBody) -> bool {
    Approval::of(body).is_some_and(|approval| approval.questions().is_some())
}

/// The blocks an event holds, if it's one that holds any.
pub(super) fn blocks_of(body: &EventBody) -> &[ContentBlock] {
    if let EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } = body { blocks } else { &[] }
}

/// What the block at `position` in an event is, wherever it's drawn. One the agent wrote keeps the key it had
/// while it streamed.
pub(super) fn block_key(seq: Seq, body: &EventBody, position: usize) -> ItemKey {
    if let EventBody::Assistant { message_id, first_index, .. } = body {
        return ItemKey::recorded_block(seq, position, message_id.as_ref(), *first_index);
    }
    ItemKey::Event(seq, position)
}

#[cfg(test)]
mod tests;
