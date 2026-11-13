use super::super::{By, Position, SessionLog};
use protocol::event::SessionEvent;
use protocol::model::{CompactionLine, Place, PromptLine, Seq};
use std::collections::BTreeMap;

/// The prompts and compactions the log knows of, by where they were recorded. They stay when their events are dropped,
/// since each is small.
#[derive(Debug, Default)]
pub(in super::super) struct Outline {
    /// Each without where it was settled, which the log keeps apart, since a line made from the prompt's event can't
    /// say, and it changes after the line is made.
    pub(super) lines: BTreeMap<Seq, PromptLine>,
    pub(super) compactions: BTreeMap<Seq, CompactionLine>,
    /// How many times `lines` or `compactions` has changed, or something moved a prompt to another row.
    pub(super) changes: u64,
    pub(super) listing: Listing,
    pub(super) compactions_listing: Listing,
}

/// Where asking the hub for one of its lists, of the session's prompts or its compactions, has got to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Listing {
    #[default]
    Wanted,
    /// The hub failed to answer, so it isn't asked again until [`SessionLog::retry_failed`].
    Failed,
    /// The hub listed them, or is too old to.
    Done,
}

impl Listing {
    pub(super) fn retry_failed(&mut self) {
        if *self == Listing::Failed {
            *self = Listing::Wanted;
        }
    }
}

impl Outline {
    /// Notes the event if it's a prompt or a compaction.
    pub(in super::super) fn saw(&mut self, event: &SessionEvent) {
        if let Some(line) = PromptLine::of(event) {
            self.add(line);
        }
        if let Some(line) = CompactionLine::of(event) {
            self.add_compaction(line);
        }
    }

    pub(super) fn add(&mut self, line: PromptLine) {
        let line = line.unsettled();
        if self.lines.get(&line.seq()) != Some(&line) {
            self.lines.insert(line.seq(), line);
            self.changes += 1;
        }
    }

    /// Where each prompt known of was recorded, newest first.
    pub(in super::super) fn newest_prompts(&self) -> impl Iterator<Item = Seq> + '_ {
        self.lines.keys().rev().copied()
    }

    /// Whether the event recorded at `seq` is a prompt the log knows of.
    pub(in super::super) fn is_prompt(&self, seq: Seq) -> bool {
        self.lines.contains_key(&seq)
    }

    /// Something moved a prompt to another row, so a view's copy of where each is is out of date.
    pub(in super::super) fn rows_moved(&mut self) {
        self.changes += 1;
    }

    pub(super) fn add_compaction(&mut self, line: CompactionLine) {
        if self.compactions.get(&line.seq) != Some(&line) {
            self.compactions.insert(line.seq, line);
            self.changes += 1;
        }
    }

    /// Lets a list the hub failed to answer be asked for again.
    pub(in super::super) fn retry_failed(&mut self) {
        self.listing.retry_failed();
        self.compactions_listing.retry_failed();
    }

    /// The log's rows now go by something else, so each prompt and compaction is in another row, and a hub that reads
    /// by place, after one that couldn't, is asked for the lists again, since the old one gave no places.
    pub(in super::super) fn moved(&mut self, by: By) {
        self.changes += 1;
        if by == By::Place {
            self.listing = Listing::Wanted;
            self.compactions_listing = Listing::Wanted;
        }
    }
}

impl SessionLog {
    /// Whether the hub is to be asked for its list of the session's prompts.
    pub(crate) fn wants_prompt_list(&self) -> bool {
        self.outline.listing == Listing::Wanted
    }

    /// Takes the hub's list of the session's prompts, which can overlap what the log has already been given, and says
    /// where each was settled, which the log may not hold.
    pub(crate) fn list_prompts(&mut self, lines: Vec<PromptLine>) {
        for line in lines {
            if let Some(at) = line.settled() {
                self.settle(line.seq(), at);
            }
            self.outline.add(line);
        }
        self.outline.listing = Listing::Done;
    }

    /// The hub is too old to list prompts, so the log keeps only those it's given.
    pub(crate) fn prompts_unlisted(&mut self) {
        self.outline.listing = Listing::Done;
    }

    /// The hub failed to list them. The log keeps those it has, and asks again on a new connection.
    pub(crate) fn prompt_list_failed(&mut self) {
        self.outline.listing = Listing::Failed;
    }

    /// Every recorded prompt known of, oldest first: each the hub listed, and each the log has been given since, live
    /// or by the page. A prompt still on its way isn't one, since it has no place in the log yet.
    pub fn prompt_lines(&self) -> impl ExactSizeIterator<Item = &PromptLine> {
        self.outline.lines.values()
    }

    /// Whether the hub is to be asked for its list of the session's compactions.
    pub(crate) fn wants_compaction_list(&self) -> bool {
        self.outline.compactions_listing == Listing::Wanted
    }

    /// Takes the hub's list of the session's compactions, which can overlap what the log has already been given.
    pub(crate) fn list_compactions(&mut self, lines: Vec<CompactionLine>) {
        for line in lines {
            self.outline.add_compaction(line);
        }
        self.outline.compactions_listing = Listing::Done;
    }

    /// The hub is too old to list compactions, so the log keeps only those it's given.
    pub(crate) fn compactions_unlisted(&mut self) {
        self.outline.compactions_listing = Listing::Done;
    }

    /// The hub failed to list them. The log keeps those it has, and asks again on a new connection.
    pub(crate) fn compaction_list_failed(&mut self) {
        self.outline.compactions_listing = Listing::Failed;
    }

    /// Every compaction of the agent's own conversation that compacted, known of, oldest first: each the hub listed,
    /// and each the log has been given since, live or by the page.
    pub fn compaction_lines(&self) -> impl ExactSizeIterator<Item = &CompactionLine> {
        self.outline.compactions.values()
    }

    /// How many times [`SessionLog::prompt_lines`] or [`SessionLog::compaction_lines`] has changed, or
    /// [`SessionLog::prompt_row`] or [`SessionLog::compaction_row`] would say other rows for them, so a view can keep
    /// its copy of them until it does.
    pub fn outline_changes(&self) -> u64 {
        self.outline.changes
    }

    /// The row a prompt is drawn in, as [`SessionLog::row_drawing`] says, or for one in its own row that isn't
    /// loaded, as [`SessionLog::row_at`] says.
    pub fn prompt_row(&self, line: &PromptLine) -> usize {
        self.row_drawing(line.seq()).unwrap_or_else(|| self.row_at(line.seq(), line.place()))
    }

    /// The row a compaction is drawn in, as [`SessionLog::row_at`] says.
    pub fn compaction_row(&self, line: &CompactionLine) -> usize {
        self.row_at(line.seq, line.place)
    }

    /// The row of the event recorded at `seq` and `place`, from a line the hub listed. A hub that lists lines without
    /// their places, as one between reading by place and listing places did, leaves the event where it is if it's
    /// loaded, and otherwise by its `seq`, which is as near as the log can tell. A hub numbers events from 1, so one it
    /// numbers 0 goes in the first row.
    pub(in super::super) fn row_at(&self, seq: Seq, place: Place) -> usize {
        let position = match self.by {
            By::Seq => Position::of_seq(seq),
            By::Place if place != Place::default() => Position::of_place(place),
            // Its `seq` stands in for its place, which is never later, since a `seq` counts every agent's events.
            By::Place => match self.row_of(seq) {
                Some(row) => return row,
                None => Position::of_seq(seq).min(self.last),
            },
        };
        position.row().unwrap_or(0)
    }
}
