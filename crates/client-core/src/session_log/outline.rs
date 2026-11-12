//! Every prompt in the session, as a line each, for finding one in a long chat, and every compaction, for marking where
//! the agent compacted. The log holds only a window of events, so these come from the hub's lists and from every
//! prompt and compaction the log has been given. Between them they miss none: the
//! subscription sends everything from where it starts, and every prompt before that was recorded before the list was
//! asked for, which is once the subscription has started. Each list is asked for once, and a hub that fails to answer
//! is asked again on a new connection, as a page is, while the log follows the session either way. A hub can be new
//! enough to list prompts and too old to list compactions, so each list goes its own way.
//!
//! A chat's rows go by the agent's own events, so a prompt or a compaction is drawn at its place among them. A hub that
//! can't read by place gives the whole log, and there each is at its `seq`.

use std::collections::BTreeMap;

use protocol::event::SessionEvent;
use protocol::model::{CompactionLine, Place, PromptLine, Seq, seq_index};

use super::{By, SessionLog};

/// The prompts and compactions the log knows of, by where they were recorded. They stay when their events are dropped,
/// since each is small.
#[derive(Debug, Default)]
pub(super) struct Outline {
    lines: BTreeMap<Seq, PromptLine>,
    compactions: BTreeMap<Seq, CompactionLine>,
    /// How many times `lines` or `compactions` has changed.
    changes: u64,
    listing: Listing,
    compactions_listing: Listing,
}

/// Where asking the hub for one of its lists, of the session's prompts or its compactions, has got to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Listing {
    #[default]
    Wanted,
    /// The hub failed to answer, so it isn't asked again until [`SessionLog::retry_failed`].
    Failed,
    /// The hub listed them, or is too old to.
    Done,
}

impl Listing {
    fn retry_failed(&mut self) {
        if *self == Listing::Failed {
            *self = Listing::Wanted;
        }
    }
}

impl Outline {
    /// Notes the event if it's a prompt or a compaction.
    pub(super) fn saw(&mut self, event: &SessionEvent) {
        if let Some(line) = PromptLine::of(event) {
            self.add(line);
        }
        if let Some(line) = CompactionLine::of(event) {
            self.add_compaction(line);
        }
    }

    fn add(&mut self, line: PromptLine) {
        if self.lines.get(&line.seq()) != Some(&line) {
            self.lines.insert(line.seq(), line);
            self.changes += 1;
        }
    }

    fn add_compaction(&mut self, line: CompactionLine) {
        if self.compactions.get(&line.seq) != Some(&line) {
            self.compactions.insert(line.seq, line);
            self.changes += 1;
        }
    }

    /// Lets a list the hub failed to answer be asked for again.
    pub(super) fn retry_failed(&mut self) {
        self.listing.retry_failed();
        self.compactions_listing.retry_failed();
    }

    /// The log's rows now go by something else, so each prompt and compaction is in another row, and a hub that reads
    /// by place, after one that couldn't, is asked for the lists again, since the old one gave no places.
    pub(super) fn moved(&mut self, by: By) {
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

    /// Takes the hub's list of the session's prompts, which can overlap what the log has already been given.
    pub(crate) fn list_prompts(&mut self, lines: Vec<PromptLine>) {
        for line in lines {
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

    /// The row a prompt is drawn in, as [`SessionLog::row_at`] says.
    pub fn prompt_row(&self, line: &PromptLine) -> usize {
        self.row_at(line.seq(), line.place())
    }

    /// The row a compaction is drawn in, as [`SessionLog::row_at`] says.
    pub fn compaction_row(&self, line: &CompactionLine) -> usize {
        self.row_at(line.seq, line.place)
    }

    /// The row of the event recorded at `seq` and `place`, from a line the hub listed. A hub that lists lines without
    /// their places, as one between reading by place and listing places did, leaves the event where it is if it's
    /// loaded, and otherwise by its `seq`, which is as near as the log can tell. A hub numbers events from 1, so one it
    /// numbers 0 goes in the first row.
    fn row_at(&self, seq: Seq, place: Place) -> usize {
        let position = match self.by {
            By::Seq => seq,
            By::Place if place != Place::default() => place.get(),
            By::Place => {
                let near = seq_index(seq).min(self.row_count());
                return self.row_of(seq).unwrap_or_else(|| near.saturating_sub(1));
            }
        };
        seq_index(position).saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::{CompactedBy, CompactionOutcome, EventBody};
    use protocol::model::Place;

    use super::*;
    use crate::session_log::tests::page;

    fn prompt(seq: Seq, text: &str) -> SessionEvent {
        let body = EventBody::UserPrompt { text: text.to_string(), client_msg_id: None, attachments: Vec::new() };
        SessionEvent {
            session_id: "s".into(),
            seq,
            ts_ms: 0,
            run: None,
            subagent: None,
            place: Place::default(),
            body,
            raw: None,
        }
    }

    fn line(seq: Seq) -> PromptLine {
        PromptLine::of(&prompt(seq, "a prompt")).unwrap()
    }

    fn seqs(log: &SessionLog) -> Vec<Seq> {
        log.prompt_lines().map(PromptLine::seq).collect()
    }

    #[test]
    fn a_prompt_recorded_after_the_list_joins_it_in_order() {
        let mut log = SessionLog::new(600);
        log.list_prompts(vec![line(3)]);

        log.apply_event(prompt(601, "a prompt"));

        assert_eq!(seqs(&log), [3, 601]);
    }

    #[test]
    fn a_prompt_both_listed_and_given_is_there_once_and_changes_nothing_the_second_time() {
        let mut log = SessionLog::new(600);
        log.apply_event(prompt(601, "a prompt"));
        let changes = log.outline_changes();

        log.list_prompts(vec![line(3), line(601)]);

        assert_eq!(seqs(&log), [3, 601]);
        assert_eq!(log.outline_changes(), changes + 1, "only the prompt it didn't know of is a change");
    }

    #[test]
    fn a_prompt_stays_listed_after_its_page_is_dropped() {
        let mut log = SessionLog::new(3000);
        let mut first = page(1..251);
        first[9] = prompt(10, "far back");
        log.focus_on(0);
        log.install_page(1, first);

        log.focus_on(2999);
        for start in (251..3000).step_by(250) {
            log.install_page(start, page(start..start + 250));
        }

        assert!(matches!(log.row(9), crate::session_log::Row::Unloaded), "its page is gone");
        assert_eq!(seqs(&log), [10]);
    }

    #[test]
    fn the_list_is_wanted_until_the_hub_answers_and_after_a_failure_only_once_its_retried() {
        let mut log = SessionLog::new(0);
        assert!(log.wants_prompt_list());

        log.prompt_list_failed();
        assert!(!log.wants_prompt_list(), "a failure isn't asked again at once");
        log.retry_failed();
        assert!(log.wants_prompt_list());

        log.list_prompts(Vec::new());
        log.retry_failed();
        assert!(!log.wants_prompt_list(), "a list once answered is never asked for again");

        let mut older = SessionLog::new(0);
        older.prompts_unlisted();
        assert!(!older.wants_prompt_list());
    }

    #[test]
    fn a_prompt_is_drawn_at_its_place_among_the_agents_own_events_and_in_the_whole_log_at_its_seq() {
        let placed = |seq, place| line(seq).placed(Place::new(place));
        let mut own = SessionLog::by_place(0);
        own.reaches(4);
        let mut whole = SessionLog::new(12);

        // After a subagent's events, so its place isn't its `seq`.
        assert_eq!(own.prompt_row(&placed(10, 4)), 3);
        assert_eq!(whole.prompt_row(&placed(10, 4)), 9);
        assert_eq!(own.prompt_row(&line(3)), 2, "a line with no place, from a hub that lists none, goes by its seq");
        assert_eq!(own.prompt_row(&line(10)), 3, "as near as the log goes");
        assert_eq!(whole.prompt_row(&line(0)), 0, "a hub numbers events from 1, so one it numbers 0 goes first");
        own.apply_event(SessionEvent { place: Place::new(5), ..prompt(12, "a prompt") });
        whole.apply_event(SessionEvent { place: Place::new(5), ..prompt(12, "a prompt") });
        let given = |log: &SessionLog| log.prompt_row(log.prompt_lines().last().unwrap());
        assert_eq!((given(&own), given(&whole)), (4, 11), "and so does one the log is given");
    }

    #[test]
    fn a_log_that_starts_again_by_place_asks_for_the_list_again_for_its_places() {
        let mut log = SessionLog::new(5);
        log.list_prompts(vec![line(3)]);
        let changes = log.outline_changes();

        log.read_by_place();

        assert!(log.wants_prompt_list(), "the hub before listed no places");
        assert!(log.outline_changes() > changes, "and its prompts are in other rows");
        assert_eq!(seqs(&log), [3], "it keeps them until the list comes");
        log.list_prompts(vec![line(3).placed(Place::new(2))]);
        log.read_whole(5);
        assert!(!log.wants_prompt_list(), "by seq, the list it has is right");
    }

    /// A compaction of the agent's own conversation at `seq` and `place`, or one that failed.
    fn compaction(seq: Seq, place: u64, compacted: bool) -> SessionEvent {
        let outcome = if compacted {
            let tokens = (Some(467_499), Some(15_694));
            let (by, took_ms, summary) = (CompactedBy::Agent, None, None);
            CompactionOutcome::Compacted { by, before_tokens: tokens.0, after_tokens: tokens.1, took_ms, summary }
        } else {
            CompactionOutcome::Failed { error: None }
        };
        let body = EventBody::AgentCompacted { asked: None, keeping: None, outcome };
        SessionEvent { place: Place::new(place), body, ..prompt(seq, "") }
    }

    fn compacted(seq: Seq, place: u64) -> CompactionLine {
        CompactionLine::of(&compaction(seq, place, true)).unwrap()
    }

    fn compaction_seqs(log: &SessionLog) -> Vec<Seq> {
        log.compaction_lines().map(|line| line.seq).collect()
    }

    #[test]
    fn a_compaction_both_listed_and_given_is_there_once_and_changes_nothing_the_second_time() {
        let mut log = SessionLog::new(600);
        log.apply_event(compaction(601, 601, true));
        let changes = log.outline_changes();

        log.list_compactions(vec![compacted(3, 3), compacted(601, 601)]);

        assert_eq!(compaction_seqs(&log), [3, 601]);
        assert_eq!(log.outline_changes(), changes + 1, "only the compaction it didn't know of is a change");
        assert_eq!(seqs(&log), Vec::<Seq>::new(), "and none is a prompt");
    }

    #[test]
    fn a_compaction_is_drawn_at_its_place_among_the_agents_own_events_and_in_the_whole_log_at_its_seq() {
        let mut own = SessionLog::by_place(0);
        own.reaches(4);
        let whole = SessionLog::new(12);

        // After a subagent's events, so its place isn't its `seq`.
        assert_eq!(own.compaction_row(&compacted(10, 4)), 3);
        assert_eq!(whole.compaction_row(&compacted(10, 4)), 9);
    }

    /// A hub can be new enough to list prompts and too old to list compactions.
    #[test]
    fn the_compactions_list_is_wanted_apart_from_the_prompts() {
        let mut log = SessionLog::new(0);
        log.list_prompts(Vec::new());
        assert!(log.wants_compaction_list(), "listing prompts doesn't list compactions");

        log.compaction_list_failed();
        assert!(!log.wants_compaction_list(), "a failure isn't asked again at once");
        log.retry_failed();
        assert!(log.wants_compaction_list());
        log.compactions_unlisted();
        log.retry_failed();
        assert!(!log.wants_compaction_list(), "a hub too old to list them isn't asked again");

        let mut log = SessionLog::new(5);
        log.list_compactions(vec![compacted(3, 0)]);
        log.read_by_place();
        assert!(log.wants_compaction_list(), "the hub before listed no places");
    }

    #[test]
    fn a_compaction_that_failed_isnt_listed() {
        let mut log = SessionLog::new(0);

        log.apply_event(compaction(2, 2, false));

        assert_eq!(log.compaction_lines().len(), 0);
        assert_eq!(log.outline_changes(), 0);
    }

    #[test]
    fn only_prompts_are_listed() {
        let mut log = SessionLog::new(0);

        log.apply_event(SessionEvent { body: EventBody::PromptTaken { prompt: 1 }, ..prompt(2, "") });

        assert_eq!(log.prompt_lines().len(), 0);
        assert_eq!(log.outline_changes(), 0);
    }
}
