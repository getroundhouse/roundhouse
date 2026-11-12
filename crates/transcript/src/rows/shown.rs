//! What each row shows, for finding text in a chat: each text it draws with a match in it, as it draws it. It walks a
//! row as [`super::render_row`] draws it, with every block open, since finding opens what a match is in, and each
//! kind of call says what it shows beside where it draws it, in `tools`.
//!
//! What's counted is what's drawn: the person's and the agent's words, what calls were given and sent back, and what
//! went wrong, but not what the app writes itself, such as how many lines a read sent back, or a closed call's summary
//! of what it opens to. What asks the person something, a card or a question's line once it's answered, and the end of
//! the chat, with the text still streaming in and the prompts the agent hasn't read yet, aren't found. A test in the
//! desktop app's gallery draws every kind of call and checks that what each draws is what's counted here.

use client_core::session_log::{Row, SessionLog};
use protocol::event::{ContentBlock, EventBody, SessionEvent, TurnOutcome};
use protocol::find::Needle;
use protocol::model::{Seq, seq_index};

use super::around::{
    Around, BlockAround, Opening, blocks_of, drawn_in_its_call, drawn_in_row, drawn_with_its_call, its_call,
};
use super::{InRow, tools};
use crate::calls;
use crate::find::{self, Part, TextId};
use crate::folds::Fold;
use crate::images_apart::images_apart;
use crate::item_key::ItemKey;
use crate::transcript::Model;

/// A text a row draws with matches in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) id: TextId,
    pub(crate) matches: usize,
    /// Whether the kit's text view draws it, as Markdown, rendered. The view the transcript keeps of it draws its
    /// matches, and brings the one being shown into view itself.
    pub(crate) kit_view: bool,
    /// The folds to open to show it, outermost first.
    pub(crate) opens: Vec<(ItemKey, Fold)>,
}

/// The texts with matches that the row at `ix` draws, in the order it draws them, from the top. A row whose event is
/// drawn in another's, such as a call's result, draws none of its own, and one that settled a prompt draws the
/// prompt.
pub(crate) fn of_row(log: &SessionLog, m: &Model, needle: &Needle, ix: usize) -> Vec<Found> {
    // A row's own, until a block it gathers says otherwise, as the prompt's text is the only one its row draws.
    let mut gather = Gather { needle, found: Vec::new(), opens: Vec::new(), in_row: InRow::Own(0) };
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
/// prompt at the end is in none that's found in.
pub(crate) fn rows_drawing(log: &SessionLog, m: &Model, position: u64) -> Vec<usize> {
    let own = seq_index(position).saturating_sub(1);
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
pub(crate) fn rows_to_count(log: &SessionLog, m: &Model, position: u64) -> Vec<usize> {
    let mut rows = rows_drawing(log, m, position);
    rows.push(seq_index(position).saturating_sub(1));
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

#[cfg(test)]
mod tests;

/// Where a row's texts with matches are gathered, what has to be opened to show the ones being gathered now, and the
/// row the block they're in is drawn in, its own or a call's, as a subagent's are.
pub(in crate::rows) struct Gather<'n> {
    needle: &'n Needle,
    found: Vec<Found>,
    opens: Vec<(ItemKey, Fold)>,
    in_row: InRow,
}

impl Gather<'_> {
    /// `text`, as it's drawn, if it has matches.
    pub(in crate::rows) fn plain(&mut self, key: &ItemKey, part: Part, text: &str) {
        self.add(key, part, text, Drawn::Plain);
    }

    /// `text`, which is recorded Markdown, as it was written, if what it renders has matches, as `super::written`
    /// draws it: from the kit's blocks in a row of its own, and with the kit's text view in a call's row.
    pub(in crate::rows) fn written(&mut self, key: &ItemKey, part: Part, text: &str) {
        let drawn = match self.in_row.own_block() {
            Some(_) => Drawn::Prose,
            None => Drawn::KitView,
        };
        self.add(key, part, text, drawn);
    }

    /// Gathers with `gather` what a block drawn in the row `in_row` says shows, and then goes back to the row it was in.
    fn in_row(&mut self, in_row: InRow, gather: impl FnOnce(&mut Self)) {
        let was = std::mem::replace(&mut self.in_row, in_row);
        gather(self);
        self.in_row = was;
    }

    /// Gathers what the block `key` shows once its `fold` is open.
    pub(in crate::rows) fn inside(&mut self, key: &ItemKey, fold: Fold, gather: impl FnOnce(&mut Self)) {
        self.opens.push((key.clone(), fold));
        gather(self);
        self.opens.pop();
    }

    fn add(&mut self, key: &ItemKey, part: Part, text: &str, drawn: Drawn) {
        let matches = match drawn {
            Drawn::Plain => self.needle.ranges(text).count(),
            Drawn::KitView | Drawn::Prose => find::markdown::count(self.needle, &images_apart(text)),
        };
        if matches > 0 {
            let (id, opens, kit_view) = ((key.clone(), part), self.opens.clone(), drawn == Drawn::KitView);
            self.found.push(Found { id, matches, kit_view, opens });
        }
    }
}

/// How a text is drawn, which is how its matches are counted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Drawn {
    Plain,
    KitView,
    Prose,
}

fn of_event(event: &SessionEvent, around: &Around, gather: &mut Gather) {
    let seq = event.seq;
    match &event.body {
        EventBody::UserPrompt { text, client_msg_id, .. } if !text.trim().is_empty() => {
            gather.written(&ItemKey::prompt(seq, client_msg_id.as_ref()), Part::Prompt, text);
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            for (ix, (block, drawing)) in blocks.iter().zip(&around.blocks).enumerate() {
                of_block(block, drawing, InRow::Own(ix), gather);
            }
        }
        EventBody::Notice { message, .. } => gather.plain(&ItemKey::Event(seq, 0), Part::Notice, message),
        EventBody::TurnEnded { outcome: TurnOutcome::Error, errors, .. } => {
            if let Some(error) = errors.first() {
                gather.plain(&ItemKey::Event(seq, 0), Part::TurnError, error);
            }
        }
        EventBody::AgentExited { reason, stderr_tail, .. } => {
            if let Some(printed) = super::exit_output(*reason, stderr_tail) {
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

/// What a block shows, as [`super::render_block`] draws it in the row `in_row` says.
pub(in crate::rows) fn of_block(block: &ContentBlock, drawing: &BlockAround, in_row: InRow, gather: &mut Gather) {
    let key = &drawing.key;
    gather.in_row(in_row, |gather| match block {
        ContentBlock::Text { text, .. } => gather.written(key, Part::Text, text),
        ContentBlock::Thinking { text } if text.is_empty() => {}
        ContentBlock::Thinking { text } => gather.inside(key, Fold::Thinking, |gather| {
            gather.written(key, Part::Thinking, text);
        }),
        ContentBlock::ToolUse { name, .. } if calls::drawn_from_its_approval(name) => {}
        ContentBlock::ToolUse { name, input, .. } => {
            tools::shown(key, name, input, &drawing.reply, drawing.subagent.as_ref(), gather);
        }
        ContentBlock::ToolResult { .. } if drawing.with_call => {}
        ContentBlock::ToolResult { .. } => {
            if let Some(answer) = drawing.reply.result() {
                tools::shown_result(key, answer, gather);
            }
        }
        ContentBlock::Image { .. } | ContentBlock::Unknown => {}
    });
}
