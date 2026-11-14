use super::super::around::{
    Around, BlockAround, Opening, blocks_of, drawn_in_its_call, drawn_in_row, drawn_with_its_call, its_call,
};
use super::super::tools;
use crate::find;
use crate::find::{Part, TextId};
use crate::folds::Fold;
use crate::images_apart::images_apart;
use crate::item_key::ItemKey;
use crate::transcript::Model;
use client_core::calls::Recorded;
use client_core::session_log::{Position, Row, SessionLog};
use protocol::event::{ContentBlock, EventBody, SessionEvent, TurnOutcome};
use protocol::find::Needle;
use protocol::model::Seq;

/// A text a row draws with matches in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) id: TextId,
    pub(crate) matches: usize,
    /// The folds to open to show it, outermost first.
    pub(crate) opens: Vec<(ItemKey, Fold)>,
}

/// The texts with matches that the row at `ix` draws, in the order it draws them, from the top. A row whose event is
/// drawn in another's, such as a call's result, draws none of its own, and one that settled a prompt draws the
/// prompt.
pub(crate) fn of_row(log: &SessionLog, m: &Model, needle: &Needle, ix: usize) -> Vec<Found> {
    let mut gather = Gather { needle, found: Vec::new(), opens: Vec::new() };
    if ix >= log.row_count() {
        return gather.found;
    }
    let Row::Event(event) = log.row(ix) else { return gather.found };
    if drawn_in_its_call(log, m, event) {
        return gather.found;
    }
    let Some(event) = drawn_in_row(log, event) else { return gather.found };
    let around = Around::read_as(log, m, event.seq, &event.body, Opening::All);
    of_event(event, &around, &mut gather);
    gather.found
}

/// The rows the event at `position` in the log is drawn in: its own, unless it's a subagent's, drawn in the call that
/// started it, or a prompt, drawn where it was settled, and the rows of the calls any of its results are drawn with. A
/// prompt at the end is in none that's found in, and so is 0, where no event is.
pub(crate) fn rows_drawing(log: &SessionLog, m: &Model, position: Position) -> Vec<usize> {
    let Some(own) = position.row() else { return Vec::new() };
    let Row::Event(event) = log.row(own) else { return vec![own] };
    if matches!(event.body, EventBody::UserPrompt { .. }) {
        return log.row_drawing(event.seq).filter(|row| *row < log.row_count()).into_iter().collect();
    }
    // A call found near it is loaded, so the log has its row.
    let row_of = |call: Seq| log.row_of(call);
    if let Some(call) = its_call(log, m, event) {
        return row_of(call).into_iter().collect();
    }
    let mut rows = vec![own];
    for block in blocks_of(&event.body) {
        if let ContentBlock::ToolResult { tool_use_id, .. } = block
            && let Some(call) = drawn_with_its_call(log, event.seq, tool_use_id).and_then(row_of)
        {
            rows.push(call);
        }
    }
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// The rows to count again when the event at `position` in the log is found to hold the needle, or changes: those
/// it's drawn in, and its own, which draws nothing of it when it's drawn in another's.
pub(crate) fn rows_to_count(log: &SessionLog, m: &Model, position: Position) -> Vec<usize> {
    let mut rows = rows_drawing(log, m, position);
    rows.extend(position.row());
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// What a row of the log counts to, with nothing chosen, for tests.
#[cfg(test)]
pub(crate) fn count(log: &SessionLog, needle: &str, ix: usize) -> usize {
    let needle = Needle::new(needle).unwrap();
    of_row(log, &Model::default(), &needle, ix).iter().map(|found| found.matches).sum()
}

/// Where a row's texts with matches are gathered, and what has to be opened to show the ones being gathered now.
pub(in crate::rows) struct Gather<'n> {
    pub(super) needle: &'n Needle,
    pub(super) found: Vec<Found>,
    pub(super) opens: Vec<(ItemKey, Fold)>,
}

impl Gather<'_> {
    /// `text`, as it's drawn, if it has matches.
    pub(in crate::rows) fn plain(&mut self, key: &ItemKey, part: Part, text: &str) {
        self.add(key, part, self.needle.ranges(text).count());
    }

    /// `text`, which is recorded Markdown, as it was written, if what it renders has matches, as `super::super::written`
    /// draws it, from the kit's blocks.
    pub(in crate::rows) fn written(&mut self, key: &ItemKey, part: Part, text: &str) {
        self.add(key, part, find::markdown::count(self.needle, &images_apart(text)));
    }

    /// Gathers what the block `key` shows once its `fold` is open.
    pub(in crate::rows) fn inside(&mut self, key: &ItemKey, fold: Fold, gather: impl FnOnce(&mut Self)) {
        self.opens.push((key.clone(), fold));
        gather(self);
        self.opens.pop();
    }

    pub(super) fn add(&mut self, key: &ItemKey, part: Part, matches: usize) {
        if matches > 0 {
            self.found.push(Found { id: (key.clone(), part), matches, opens: self.opens.clone() });
        }
    }
}

fn of_event(event: &SessionEvent, around: &Around, gather: &mut Gather) {
    let seq = event.seq;
    match &event.body {
        EventBody::UserPrompt { client_msg_id, prompt, .. } if prompt.says_anything() => {
            gather.written(&ItemKey::prompt(seq, client_msg_id.as_ref()), Part::Prompt, &prompt.text);
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            for (block, drawing) in blocks.iter().zip(&around.blocks) {
                of_block(block, drawing, gather);
            }
        }
        EventBody::Notice { message, .. } => gather.written(&ItemKey::Event(seq, 0), Part::Notice, message),
        EventBody::RequestFailed { failure, .. } => {
            gather.written(&ItemKey::Event(seq, 0), Part::FailedRequest, &failure.message);
        }
        // A turn that ended on a failed request draws nothing, since the request's card says why.
        EventBody::TurnEnded { outcome: TurnOutcome::Error, errors, request_failed: false, .. } => {
            if let Some(error) = errors.first() {
                gather.plain(&ItemKey::Event(seq, 0), Part::TurnError, error);
            }
        }
        EventBody::AgentExited { reason, stderr_tail, .. } => {
            if let Some(printed) = super::super::exit_output(*reason, stderr_tail) {
                gather.plain(&ItemKey::Event(seq, 0), Part::Exit, printed);
            }
        }
        EventBody::UserPrompt { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentMode { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentContext { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        // What a compaction says, and the summary it opens to, aren't found.
        | EventBody::CompactionAsked { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => {}
    }
}

/// What a block shows, as [`super::super::render_block`] draws it.
pub(in crate::rows) fn of_block(block: &ContentBlock, drawing: &BlockAround, gather: &mut Gather) {
    let key = &drawing.key;
    match block {
        ContentBlock::Text { text, .. } => gather.written(key, Part::Text, text),
        ContentBlock::Thinking { text } if text.is_empty() => {}
        ContentBlock::Thinking { text } => gather.inside(key, Fold::Thinking, |gather| {
            gather.written(key, Part::Thinking, text);
        }),
        ContentBlock::ToolUse { .. } => {
            // One that asks the person something is drawn from its approval, which has everything that was asked.
            if let Some(recorded) = Recorded::of(block).filter(|call| !call.asks_the_person()) {
                tools::shown(key, recorded, &drawing.reply, drawing.subagent.as_ref(), gather);
            }
        }
        ContentBlock::ToolResult { .. } if drawing.with_call => {}
        ContentBlock::ToolResult { .. } => {
            if let Some(answer) = drawing.reply.result() {
                tools::shown_result(key, answer, gather);
            }
        }
        ContentBlock::Image { .. }
        | ContentBlock::Call { .. }
        | ContentBlock::CallResult { .. }
        | ContentBlock::Unknown => {}
    }
}
