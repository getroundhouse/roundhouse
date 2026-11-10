//! Every prompt in the session, as a line each, for finding one in a long chat. The log holds only a window of events,
//! so these come from the hub's list and from every prompt the log has been given. Between them they miss none: the
//! subscription sends everything from where it starts, and every prompt before that was recorded before the list was
//! asked for, which is once the subscription has started. The list is asked for once, and a hub that fails to answer
//! is asked again on a new connection, as a page is, while the log follows the session either way.

use std::collections::BTreeMap;

use protocol::event::SessionEvent;
use protocol::model::{PromptLine, Seq};

use super::SessionLog;

/// The prompts the log knows of, by where they were recorded. They stay when their events are dropped, since each is
/// small.
#[derive(Debug, Default)]
pub(super) struct Outline {
    lines: BTreeMap<Seq, PromptLine>,
    /// How many times `lines` has changed.
    changes: u64,
    listing: Listing,
}

/// Where asking the hub for its list of the session's prompts has got to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Listing {
    #[default]
    Wanted,
    /// The hub failed to answer, so it isn't asked again until [`SessionLog::retry_failed`].
    Failed,
    /// The hub listed them, or is too old to.
    Done,
}

impl Outline {
    /// Notes the event if it's a prompt.
    pub(super) fn saw(&mut self, event: &SessionEvent) {
        if let Some(line) = PromptLine::of(event.seq, &event.body) {
            self.add(line);
        }
    }

    fn add(&mut self, line: PromptLine) {
        if self.lines.get(&line.seq()) != Some(&line) {
            self.lines.insert(line.seq(), line);
            self.changes += 1;
        }
    }

    /// Lets a list the hub failed to answer be asked for again.
    pub(super) fn retry_failed(&mut self) {
        if self.listing == Listing::Failed {
            self.listing = Listing::Wanted;
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

    /// How many times [`SessionLog::prompt_lines`] has changed, so a view can keep its copy of them until it does.
    pub fn prompt_lines_changed(&self) -> u64 {
        self.outline.changes
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::EventBody;
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
        PromptLine::of(seq, &prompt(seq, "a prompt").body).unwrap()
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
        let changes = log.prompt_lines_changed();

        log.list_prompts(vec![line(3), line(601)]);

        assert_eq!(seqs(&log), [3, 601]);
        assert_eq!(log.prompt_lines_changed(), changes + 1, "only the prompt it didn't know of is a change");
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
    fn only_prompts_are_listed() {
        let mut log = SessionLog::new(0);

        log.apply_event(SessionEvent { body: EventBody::PromptTaken { prompt: 1 }, ..prompt(2, "") });

        assert_eq!(log.prompt_lines().len(), 0);
        assert_eq!(log.prompt_lines_changed(), 0);
    }
}
