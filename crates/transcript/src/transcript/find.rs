//! Finding text in a chat, from the bar `Cmd-F` opens over it, as `docs/spec/chats.md` says. The transcript holds a
//! window of the log, so it counts what the rows it holds draw itself, whenever they change, and client-core's search
//! reads the rest of the chat around each event the hub says holds the text, for it to count what the other rows
//! draw. Which match is shown, and what the bar says, is the device's own, as where the chat is scrolled to is.
//!
//! A row is counted from what draws it: one the log holds from the log, as it's drawn, and one it doesn't from the
//! stretch of the chat read around it, until it loads. Events are known by their position in the log, which is their
//! place among the agent's own, or their `seq` in a log of every event, from a hub too old to find.

use std::collections::BTreeSet;
use std::ops::Range;
use std::rc::Rc;

use client_core::session_log::{Row, SessionLog};
use client_core::{Client, Search, Searched};
use gpui_kit::{Context, Task, Window};
use protocol::find::Needle;

use super::{FoundMarkdown, Model, Transcript, TranscriptEvent};
use crate::find::FindLook;
use crate::find::reveal::Reveal;
use crate::rows::{NEAR, REACH, found_in_row, rows_drawing, rows_to_count};
use bar::Bar;
use count::{At, Counts};
use stepping::Revealing;

mod bar;
mod count;
mod stepping;
mod stretch;
#[cfg(feature = "testing")]
mod testing;

/// Where finding has got to in one chat, while its bar is open.
pub(crate) struct Finding {
    bar: Bar,
    /// What's typed, once it's been looked for.
    query: Option<Query>,
    /// Where the match being shown was drawn, which rows are drawn with.
    reveal: Reveal,
    revealing: Revealing,
}

/// Something typed, being looked for. Each is new, so nothing found for one typed before is kept.
struct Query {
    needle: Needle,
    search: Search,
    counts: Counts,
    /// Where every event known to hold the needle is in the log: those the log holds that do, and those the hub
    /// named.
    candidates: BTreeSet<u64>,
    /// The match being shown.
    at: Option<At>,
    /// A match whose row is loading, to show once it has.
    going_to: Option<At>,
    /// Whether the first match is still to be shown, once it's known which it is.
    first: bool,
    /// Counting the last stretch the search read, on another thread, since it can be a thousand events.
    counting: Option<Task<()>>,
}

impl Query {
    fn new(needle: Needle, client: &Client, session: &client_core::OpenSession) -> Self {
        Self {
            search: session.search(client, needle.clone(), NEAR as u64),
            needle,
            counts: Counts::default(),
            candidates: BTreeSet::new(),
            at: None,
            going_to: None,
            first: true,
            counting: None,
        }
    }

    /// Whether every match is counted, with the whole chat in the log if the hub wasn't asked or couldn't say.
    fn complete(&self, everything_held: bool) -> bool {
        match self.searched() {
            Searched::Done => true,
            Searched::Underway => false,
            Searched::TooShort | Searched::CantFind | Searched::Failed => everything_held,
        }
    }

    /// How far the search has got, with what it's read counted.
    fn searched(&self) -> Searched {
        if self.counting.is_some() { Searched::Underway } else { self.search.searched() }
    }

    /// What the bar says of the matches.
    fn says(&self, everything_held: bool) -> String {
        let more = match self.searched() {
            _ if self.complete(everything_held) => More::No,
            Searched::Underway => More::Counting,
            Searched::Done | Searched::TooShort | Searched::CantFind | Searched::Failed => More::Unlooked,
        };
        says(self.at.and_then(|at| self.counts.place(at)), self.counts.total(), more)
    }
}

/// Whether there may be more matches than are counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum More {
    No,
    /// They're still being counted.
    Counting,
    /// Only what the log holds was looked through.
    Unlooked,
}

/// What the bar says of `total` matches, with the one shown at `place`, counting from 0: `…` while there may be more
/// counted, and `+` when there may be more that weren't looked for.
fn says(place: Option<usize>, total: usize, more: More) -> String {
    let mark = match more {
        More::No => "",
        More::Counting => "…",
        More::Unlooked => "+",
    };
    match (place, more) {
        (Some(place), _) => format!("{} of {total}{mark}", place + 1),
        (None, _) if total > 0 => format!("{total} found{mark}"),
        (None, More::No) => "No matches".to_string(),
        (None, More::Counting) => "Finding…".to_string(),
        (None, More::Unlooked) => "No matches loaded".to_string(),
    }
}

impl Transcript {
    /// Opens the bar, or brings focus back to it with what's in it selected.
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.finding.is_none() {
            let bar = Bar::new(window, cx);
            self.finding =
                Some(Finding { bar, query: None, reveal: Reveal::default(), revealing: Revealing::default() });
        }
        if let Some(finding) = &self.finding {
            finding.bar.focus(window, cx);
        }
        cx.notify();
    }

    /// Whether the bar is open.
    pub fn is_finding(&self) -> bool {
        self.finding.is_some()
    }

    /// What the bar says of the matches, while it's open.
    pub fn find_says(&self) -> Option<String> {
        let finding = self.finding.as_ref()?;
        let everything_held = self.session.read(SessionLog::holds_every_event);
        Some(finding.query.as_ref().map(|query| query.says(everything_held)).unwrap_or_default())
    }

    /// The row the match being shown is in.
    pub fn found_row(&self) -> Option<usize> {
        self.query().and_then(|query| query.at).map(|at| at.row)
    }

    /// Closes the bar, and the matches go. A call opened to show one stays open.
    pub fn close_find(&mut self, cx: &mut Context<Self>) {
        if self.finding.take().is_none() {
            return;
        }
        let mut m = self.model.borrow_mut();
        m.find = Rc::default();
        m.found_markdown.clear();
        drop(m);
        self.texts.forget_found(cx);
        cx.emit(TranscriptEvent::FindClosed);
        cx.notify();
    }

    /// Looks for what's typed in the bar now, from the start.
    pub(crate) fn find_again(&mut self, cx: &mut Context<Self>) {
        let Some(finding) = &mut self.finding else { return };
        let needle = Needle::new(&finding.bar.query(cx)).ok();
        finding.reveal.done();
        finding.query = needle.clone().map(|needle| Query::new(needle, &self.client, &self.session));
        {
            self.texts.forget_found(cx);
            let mut m = self.model.borrow_mut();
            m.found_markdown.clear();
            m.find = Rc::new(FindLook::new(needle.clone(), None, finding.reveal.clone()));
        }
        if let Some(needle) = needle {
            let (found, rows) = {
                let m = self.model.borrow();
                self.session.read(|log| held_to_count(log, &m, &needle))
            };
            if let Some(query) = self.query_mut() {
                query.candidates.extend(found);
            }
            self.count_rows(rows, cx);
        }
        self.after_counting(cx);
    }

    /// Carries on on a new connection, asking again of a search that failed, or of a hub that couldn't find, since it
    /// may be a newer one.
    pub(crate) fn find_on_new_connection(&mut self) {
        let (client, session) = (&self.client, &self.session);
        let Some(query) = self.finding.as_mut().and_then(|finding| finding.query.as_mut()) else { return };
        if matches!(query.search.searched(), Searched::Failed | Searched::CantFind) {
            query.search = session.search(client, query.needle.clone(), NEAR as u64);
        }
    }

    /// Counts again what each of `rows` the log holds draws, as it draws it, and notes the Markdown with matches in
    /// each, for the row to keep views of once it's drawn. A row the log doesn't hold keeps what it was counted as.
    fn count_rows(&mut self, rows: BTreeSet<usize>, cx: &mut Context<Self>) {
        let Some(needle) = self.query().map(|query| query.needle.clone()) else { return };
        let counted = {
            let mut m = self.model.borrow_mut();
            let counted = self.session.read(|log| count_held(log, &m, &needle, rows));
            for (row, _, texts) in &counted {
                if texts.is_empty() {
                    m.found_markdown.remove(row);
                } else {
                    m.found_markdown.insert(*row, texts.clone());
                }
            }
            counted
        };
        if let Some(query) = self.query_mut() {
            for (row, count, _) in counted {
                query.counts.set(row, count);
            }
        }
        cx.notify();
    }

    /// Follows what changed in the log's `changed` rows. Each event there that holds the needle, and each row there
    /// with matches, is counted again from what's held now, and so is each row there that draws an event after them,
    /// such as a call whose result has just come, or a subagent's work.
    pub(crate) fn find_in_changes(&mut self, changed: Range<usize>, cx: &mut Context<Self>) {
        let Some(query) = self.query() else { return };
        let (found, rows) = {
            let m = self.model.borrow();
            self.session.read(|log| {
                changed_to_count(log, &m, (&query.needle, &query.candidates, &query.counts), changed.clone())
            })
        };
        if let Some(query) = self.query_mut() {
            query.candidates.extend(found);
        }
        self.count_rows(rows, cx);
        self.after_counting(cx);
    }

    /// Counts the next stretch the search has read, unless it's counting one, and says if the search failed.
    pub(crate) fn find_news(&mut self, cx: &mut Context<Self>) {
        let model = self.model.clone();
        let settled = self.session.read(SessionLog::settled_prompts);
        let Some(query) = self.query_mut() else { return };
        if query.counting.is_none()
            && let Some(stretch) = query.search.take()
        {
            // The subagents' records, which draw what their calls show.
            let records = model.borrow().subagents.clone();
            let known = stretch::Known { records, settled };
            query.counting = Some(Self::count_stretch(stretch, query.needle.clone(), known, cx));
        }
        if let Some(e) = query.search.take_error() {
            cx.emit(TranscriptEvent::Error(format!("couldn't find in the whole chat: {e}")));
        }
        self.after_counting(cx);
    }

    /// Once counts have changed: shows the first match if it's due and known, keeps the one shown as near as it can
    /// be, and draws again. The first is the first at or below the top of the chat, once nothing between the top and
    /// it can hold another: once every row there is held, or the search can't say more.
    fn after_counting(&mut self, cx: &mut Context<Self>) {
        let top = self.list.top().row;
        let everything_held = self.session.read(SessionLog::holds_every_event);
        let session = self.session.clone();
        let Some(query) = self.query_mut() else { return };
        if query.first {
            let settled = query.complete(everything_held) || query.searched() != Searched::Underway;
            let first = session.read(|log| first_match(&query.counts, top, settled, log));
            if let Some(first) = first {
                query.first = false;
                self.select(first, cx);
            }
        } else if let Some(at) = query.at {
            query.at = query.counts.kept(at);
        }
        cx.notify();
    }

    fn query(&self) -> Option<&Query> {
        self.finding.as_ref().and_then(|finding| finding.query.as_ref())
    }

    fn query_mut(&mut self) -> Option<&mut Query> {
        self.finding.as_mut().and_then(|finding| finding.query.as_mut())
    }
}

/// Where the events the log holds that hold the needle are, and the rows that draw them, to count when something's
/// typed.
fn held_to_count(log: &SessionLog, m: &Model, needle: &Needle) -> (Vec<u64>, BTreeSet<usize>) {
    let holding = log.loaded().filter(|event| needle.is_in_event(&event.body));
    let found: Vec<u64> = holding.map(|event| log.position(event)).collect();
    let rows = found.iter().flat_map(|position| rows_to_count(log, m, *position)).collect();
    (found, rows)
}

/// What to count again once the log's `changed` rows have changed: the events there that hold the needle, and the rows
/// to count. Those are the rows there with matches, the rows that draw the events there that hold it, and the rows of
/// any events after them that hold it and are drawn in a row there, such as a subagent's work in the call that started
/// it.
fn changed_to_count(
    log: &SessionLog,
    m: &Model,
    (needle, candidates, counts): (&Needle, &BTreeSet<u64>, &Counts),
    changed: Range<usize>,
) -> (Vec<u64>, BTreeSet<usize>) {
    let mut rows: BTreeSet<usize> = counts.rows_in(changed.clone()).collect();
    let mut found = Vec::new();
    for ix in changed.start..changed.end.min(log.row_count()) {
        let Row::Event(event) = log.row(ix) else { continue };
        let position = ix as u64 + 1;
        if candidates.contains(&position) || needle.is_in_event(&event.body) {
            found.push(position);
            rows.extend(rows_to_count(log, m, position));
        }
    }
    let after = changed.end as u64 + 1..(changed.end + 1 + REACH) as u64;
    for &position in candidates.range(after) {
        let drawn_here = || rows_drawing(log, m, position).iter().any(|row| changed.contains(row));
        if is_loaded(log, row_at(position)) && drawn_here() {
            rows.extend(rows_to_count(log, m, position));
        }
    }
    (found, rows)
}

/// The row of the event at `position` in the log.
fn row_at(position: u64) -> usize {
    usize::try_from(position.saturating_sub(1)).unwrap_or(usize::MAX)
}

/// What each of `rows` the log holds counts to, as it's drawn, with the Markdown in it that has matches. A row the log
/// doesn't hold isn't counted here, and keeps what it was counted as.
fn count_held(
    log: &SessionLog,
    m: &Model,
    needle: &Needle,
    rows: BTreeSet<usize>,
) -> Vec<(usize, usize, FoundMarkdown)> {
    rows.into_iter()
        .filter(|row| is_loaded(log, *row))
        .map(|row| {
            let found = found_in_row(log, m, needle, row);
            let count = found.iter().map(|found| found.matches).sum();
            let markdown = found.into_iter().filter(|found| found.kit_view).map(|found| found.id).collect();
            (row, count, markdown)
        })
        .collect()
}

/// The first match to show: the first at or below `top`, once nothing between `top` and it can hold another, because
/// every row there is held or the count is `settled`, and with none below once it's settled, the first of all.
fn first_match(counts: &Counts, top: usize, settled: bool, log: &SessionLog) -> Option<At> {
    match counts.rows().find(|row| *row >= top) {
        Some(row) if settled || log.holds(top as u64 + 1..row as u64 + 1) => Some(At { row, nth: 0 }),
        None if settled => counts.first_from(top),
        Some(_) | None => None,
    }
}

/// Whether the row at `ix` is an event the log holds.
fn is_loaded(log: &SessionLog, ix: usize) -> bool {
    matches!(log.row(ix), Row::Event(_))
}

#[cfg(test)]
mod tests;
