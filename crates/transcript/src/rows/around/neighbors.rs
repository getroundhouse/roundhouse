use super::{nearby, reply, settled, subagent};
use crate::attachments;
use crate::attachments::Carried;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::transcript::{AtWork, IdleCache, Model};
use client_core::calls::Recorded;
use client_core::session_log::{PromptAt, PromptState, SessionLog};
use client_core::{Approval, ToolUseAsked};
use gpui_kit::SharedString;
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, Question, SessionEvent};
use protocol::model::{ApprovalId, PermissionMode, Seq, ToolUseId};
use reply::{Answer, Reply};
use settled::Settled;
use subagent::Subagent;

/// How many events apart a call and its result can be and still be drawn together. A result follows its call
/// within a few events. One further away is drawn where it is.
pub(crate) const NEAR: usize = 64;

pub(in super::super) struct Around {
    /// What each of the event's blocks needs, by position.
    pub(in super::super) blocks: Vec<BlockAround>,
    /// For an approval: whether it's still waiting.
    pub(in super::super) approval_open: bool,
    /// For a question that's no longer waiting, which is drawn as a call's line.
    pub(in super::super) asked: Option<Asked>,
    /// For the record that closes an approval: what else says what became of it.
    pub(in super::super) closes: Closes,
    /// For a prompt: the files it carries.
    pub(in super::super) carried: Vec<Carried>,
    /// For a prompt: whether the agent has it.
    pub(in super::super) prompt: PromptState,
    /// For a turn's end: what the agent the chat runs is called, which its card names, if the hub has said.
    pub(in super::super) agent: Option<SharedString>,
    /// For the newest turn's end: what the cache holds of the conversation while it can expire, which its line and
    /// card say. An older turn's say nothing of it, since only the newest can still be warm.
    pub(in super::super) cache: Option<IdleCache>,
    /// For a change of mode: the mode before it, when the log says.
    pub(in super::super) mode_before: Option<PermissionMode>,
    /// For a compaction: whether it's open to its summary.
    pub(in super::super) open: bool,
    /// For a failed request: whether its card offers Retry, as [`retry_at`] says.
    pub(in super::super) retry: bool,
}

/// What says what became of an approval, besides the record that closes it.
#[derive(Debug, PartialEq, Eq)]
pub(in super::super) enum Closes {
    /// Nothing else does, or the request isn't loaded.
    Unsaid,
    /// Something drawn already does: a question's line, or, for a denial, the call it asked to make, whose result says
    /// it was denied.
    SaidElsewhere,
    /// Nothing in the chat does, since it asked to use a tool for a subagent, whose call is in the subagent's work: the
    /// tool, and what the subagent is called.
    ForASubagent { tool: String, subagent: String },
}

impl Closes {
    /// For the record at `seq` that closes the approval `id` with `outcome`.
    pub(super) fn read(log: &SessionLog, m: &Model, seq: Seq, id: &ApprovalId, outcome: ApprovalOutcome) -> Self {
        let asked = |event: &&SessionEvent| {
            let EventBody::ApprovalRequested { approval_id, .. } = &event.body else { return false };
            approval_id == id
        };
        let Some(request) = nearby::events_before(log, seq, NEAR).find(asked) else { return Closes::Unsaid };
        if asks_questions(&request.body) {
            return Closes::SaidElsewhere;
        }
        let Some(asked) = ToolUseAsked::of(&request.body) else { return Closes::Unsaid };
        if let Some(subagent) = asked.asked_by {
            return Closes::ForASubagent { tool: asked.tool, subagent: m.subagent_called(subagent) };
        }
        // A call's line says only that it was denied, so a request cancelled or allowed says so itself, whatever its
        // call's result says, as one from a log an adapter wrote wrongly might.
        let EventBody::ApprovalRequested { tool_use_id: Some(call), .. } = &request.body else { return Closes::Unsaid };
        if outcome != ApprovalOutcome::Denied {
            return Closes::Unsaid;
        }
        let denied = |block: &ContentBlock| {
            let ContentBlock::ToolResult { tool_use_id, denied, .. } = block else { return false };
            tool_use_id == call && denied.is_some()
        };
        let said = nearby::events_after(log, seq, NEAR).any(|event| blocks_of(&event.body).iter().any(denied));
        if said { Closes::SaidElsewhere } else { Closes::Unsaid }
    }
}

/// What drawing a question that's no longer waiting needs.
pub(in super::super) struct Asked {
    /// The questions it asked, in order.
    pub(in super::super) questions: Vec<Question>,
    /// What it is, as its event's only item.
    pub(in super::super) key: ItemKey,
    pub(in super::super) open: bool,
    pub(in super::super) settled: Settled,
}

/// What drawing one block of an event needs.
pub(in super::super) struct BlockAround {
    /// What the block is, wherever it's drawn.
    pub(in super::super) key: ItemKey,
    /// The kind of fold it is, and whether that's open.
    pub(in super::super) fold: Fold,
    pub(in super::super) open: bool,
    /// For a call: what's come back for it. For a result drawn on its own: itself.
    pub(in super::super) reply: Reply,
    /// For a call that handed work to a subagent: what the subagent has done.
    pub(in super::super) subagent: Option<Subagent>,
    /// For a result: whether it's drawn with its call instead.
    pub(in super::super) with_call: bool,
}

/// Which blocks are read as open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Opening {
    /// As they're drawn: as the person left them, or as their kind starts.
    AsChosen,
    /// Every one, as finding reads them, since it opens what a match is in.
    All,
}

/// The failed request whose card offers Retry, if one does: the one the newest turn ended on, while the record says
/// that turn failed and no turn has started since, and while nothing has been sent, which Retry would only repeat.
pub(crate) fn retry_at(log: &SessionLog, at_work: &AtWork) -> Option<Seq> {
    if !at_work.failed || log.has_prompts_waiting() {
        return None;
    }
    nearby::failed_request_ending(log, at_work.reply_seq)
}

impl Around {
    pub(in super::super) fn read(log: &SessionLog, m: &Model, seq: Seq, body: &EventBody) -> Self {
        Self::read_as(log, m, seq, body, Opening::AsChosen)
    }

    pub(in super::super) fn read_as(log: &SessionLog, m: &Model, seq: Seq, body: &EventBody, opening: Opening) -> Self {
        let blocks = blocks_of(body)
            .iter()
            .enumerate()
            .map(|(position, block)| BlockAround::read(log, m, seq, block, block_key(seq, body, position), opening))
            .collect();
        let mut around = Around {
            blocks,
            approval_open: false,
            asked: None,
            closes: Closes::Unsaid,
            carried: Vec::new(),
            prompt: PromptState::Taken,
            agent: m.agent.clone(),
            cache: (seq == m.at_work.reply_seq).then(|| m.at_work.cache.clone()).flatten(),
            mode_before: None,
            open: false,
            retry: m.retry_at == Some(seq),
        };
        if let EventBody::AgentMode { .. } = body {
            around.mode_before = nearby::mode_before(log, seq);
        }
        if matches!(body, EventBody::AgentCompacted { .. }) {
            around.open = m.folds.is_open(&ItemKey::Event(seq, 0), Fold::Output);
        }
        if let EventBody::UserPrompt { prompt, .. } = body {
            around.carried = attachments::carried(m, &prompt.files);
            around.prompt = log.prompt_state(seq);
        }
        if let EventBody::ApprovalRequested { approval_id, .. } = body {
            around.approval_open = log
                .open_approvals(m.approvals.as_ref())
                .iter()
                .any(|e| matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id));
            let questions = Approval::of(body).and_then(|approval| approval.questions().map(<[Question]>::to_vec));
            if let Some(questions) = questions.filter(|_| !around.approval_open) {
                let key = ItemKey::Event(seq, 0);
                let open = m.folds.is_open(&key, Fold::Output);
                around.asked = Some(Asked { questions, key, open, settled: Settled::read(log, seq, approval_id) });
            }
        }
        if let EventBody::ApprovalResolved { approval_id, outcome, .. } = body {
            around.closes = Closes::read(log, m, seq, approval_id, *outcome);
        }
        around
    }

    /// Every image the event draws now, for its row to ask the hub for: a prompt's, and those of each call or result
    /// that's open, including what a call gathered from its subagent, or the calls it shows from its subagent's record.
    /// A closed call's images wait until it's opened.
    pub(in super::super) fn images(&self) -> Vec<&Carried> {
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
    pub(in super::super) fn read(
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
        // Drawn with its call when the call is close enough before it to have found it.
        let with_call = matches!(block, ContentBlock::ToolResult { tool_use_id, .. } | ContentBlock::CallResult { tool_use_id, .. }
            if nearby::call_before(log, seq, tool_use_id, NEAR).is_some());
        let (reply, subagent) = match block {
            ContentBlock::ToolUse { id, .. } | ContentBlock::Call { id, .. } => {
                let hands_off = Recorded::of(block).is_some_and(|call| call.hands_off());
                let subagent = hands_off.then(|| Subagent::read(log, m, seq, id, open, opening));
                (Reply::to(log, m, seq, id), subagent)
            }
            ContentBlock::ToolResult { .. } | ContentBlock::CallResult { .. } if !with_call => {
                (Reply::of(block, m), None)
            }
            ContentBlock::ToolResult { .. }
            | ContentBlock::CallResult { .. }
            | ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => (Reply::Nothing, None),
        };
        Self { key, fold, open, reply, subagent, with_call }
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
pub(in super::super) fn asks_questions(body: &EventBody) -> bool {
    Approval::of(body).is_some_and(|approval| approval.questions().is_some())
}

/// The blocks an event holds, if it's one that holds any.
pub(in super::super) fn blocks_of(body: &EventBody) -> &[ContentBlock] {
    if let EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } = body { blocks } else { &[] }
}

/// What the block at `position` in an event is, wherever it's drawn. One the agent wrote keeps the key it had
/// while it streamed.
pub(in super::super) fn block_key(seq: Seq, body: &EventBody, position: usize) -> ItemKey {
    if let EventBody::Assistant { message_id, first_index, .. } = body {
        return ItemKey::recorded_block(seq, position, message_id.as_ref(), *first_index);
    }
    ItemKey::Event(seq, position)
}
