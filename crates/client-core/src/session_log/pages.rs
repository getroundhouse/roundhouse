//! Which stretches of a session's log a client fetches, keeps and lets go of, a page of [`PAGE_EVENTS`] at a time,
//! around the range a view needs and at the end, which a subscription delivers, and what's asked for again after a fetch
//! fails.

use std::ops::Range;

use protocol::event::{EventBody, SessionEvent};
use protocol::model::SettledAt;

use super::{MAX_LOADED_EVENTS, Position, SessionLog};

pub(super) const PAGE_EVENTS: usize = 250;

impl SessionLog {
    pub(crate) fn focused_on(&self, rows: &Range<usize>, unfetched: Option<usize>) -> bool {
        self.focus == *rows && self.unfetched == unfetched
    }

    /// Protects the whole viewport and its buffer, including rows already fetched. Returns whether moving it let
    /// go of events, which the view must hear about even when no new page needs fetching.
    pub(crate) fn focus_on(&mut self, rows: Range<usize>, unfetched: Option<usize>) -> bool {
        self.focus = rows;
        self.unfetched = unfetched;
        let before = self.events.len();
        self.evict();
        self.events.len() != before
    }

    /// The next batch for the range a view needs, bottom first, marking it as in flight. A page the subscription has
    /// yet to deliver isn't one of them. The view wakes the fetcher again while more pages are needed.
    pub(crate) fn pages_to_fetch(&mut self) -> Vec<Range<Position>> {
        let wanted: Vec<_> = self.wanted_pages().filter(|page| self.needs_page(*page)).collect();
        self.in_flight.extend(&wanted);
        wanted.into_iter().map(page_range).collect()
    }

    /// Whether waking the fetcher can do anything. Reporting a stationary viewport must not copy the log or wake
    /// its thread on every frame once its pages are loaded, on their way, or waiting for a failed fetch to retry.
    pub(crate) fn wants_pages(&self) -> bool {
        self.wanted_pages().any(|page| self.needs_page(page))
    }

    fn wanted_pages(&self) -> impl Iterator<Item = usize> {
        let last = page_of(self.last);
        // Fetch around the missing row the frame actually drew, not every page its estimates put in sight. The
        // live tail or a long text can still be arriving. Reconsider after at most three pages, when their real
        // heights can narrow the viewport. Loaded rows only need protection, except when a new hub made them stale.
        let asked = self.unfetched.map(|row| page_of(Position::of_row(row).min(self.last)));
        let stale = || self.stale.range(self.focus_pages()).next_back().copied();
        asked.or_else(stale).into_iter().flat_map(move |page| {
            [Some(page), page.checked_sub(1), (page < last).then_some(page + 1)].into_iter().flatten()
        })
    }

    fn needs_page(&self, page: usize) -> bool {
        let fetchable = self.row_count() > 0
            && page_range(page).start <= self.received
            && !self.failed.contains(&page)
            && !self.in_flight.contains(&page);
        let loaded = self.pages.contains(&page) && !self.stale.contains(&page);
        fetchable && !loaded
    }

    fn focus_pages(&self) -> Range<usize> {
        let first = page_of(Position::of_row(self.focus.start).min(self.last));
        let last = page_of(Position::of_row(self.focus.end.saturating_sub(1)).min(self.last));
        first..last.max(first) + 1
    }

    /// Puts a fetched range in place.
    pub(crate) fn install_page(&mut self, start: Position, events: Vec<SessionEvent>) {
        let page = page_of(start);
        self.in_flight.remove(&page);
        self.stale.remove(&page);
        self.pages.insert(page);
        let (mut settles, mut prompts) = (Vec::new(), Vec::new());
        for event in events {
            let position = self.position(&event);
            // One at 0 has no row, as with a live one.
            let Some(_) = position.row() else { continue };
            self.last = self.last.max(position);
            self.outline.saw(&event);
            settles.extend(SettledAt::of(&event));
            prompts.extend(matches!(event.body, EventBody::UserPrompt { .. }).then_some(event.seq));
            self.insert(event);
        }
        self.events_added += 1;
        self.evict();
        let first = page * PAGE_EVENTS;
        self.touched(first..(first + PAGE_EVENTS).min(self.row_count()));
        // A prompt is drawn where it was settled, which can be in another page.
        for (prompt, at) in settles {
            self.settle(prompt, at);
        }
        for prompt in prompts {
            self.prompt_moved(prompt, None);
        }
    }

    /// A fetch failed, so the pages and requests still on their way won't come. They aren't asked for again until
    /// [`SessionLog::retry_failed`], so a view looking at them doesn't keep asking.
    pub(crate) fn fetch_failed(&mut self) {
        self.failed.append(&mut self.in_flight);
        self.apart.fetch_failed();
    }

    /// Lets failed pages and requests, and a failed list of prompts, be asked for again, such as on a new connection.
    pub(crate) fn retry_failed(&mut self) {
        self.failed.clear();
        self.outline.retry_failed();
        self.apart.retry_failed();
    }

    /// Keeps `event`, which came live at `position`, if it's in the newest page or the one before, which stay loaded
    /// from live events. Anything older is fetched when it's scrolled to.
    pub(super) fn keep_live(&mut self, position: Position, event: SessionEvent) {
        if page_of(position) + 1 >= page_of(self.last) {
            self.pages.insert(page_of(position));
            self.insert(event);
            self.events_added += 1;
            self.evict();
        }
    }

    /// Drops whole pages furthest from where a view is looking, keeping the tail for live updates.
    fn evict(&mut self) {
        let focus = self.focus_pages();
        let protected = focus.start.saturating_sub(1)..focus.end.saturating_add(1);
        let tail = page_of(self.last);
        while self.events.len() > MAX_LOADED_EVENTS {
            let far = self
                .pages
                .iter()
                .copied()
                .filter(|page| !protected.contains(page) && page.abs_diff(tail) > 1)
                .max_by_key(|page| page.abs_diff(focus.start).min(page.abs_diff(focus.end - 1)));
            let Some(far) = far else { return };
            self.pages.remove(&far);
            let stale = self.stale.remove(&far);
            let doomed: Vec<Position> = self.events.range(page_range(far)).map(|(position, _)| *position).collect();
            for position in doomed {
                if let Some(event) = self.events.remove(&position) {
                    self.positions.remove(&event.seq);
                    // A request that waits stays, so what waits is still asked.
                    self.apart.keep_if_waiting(event, stale);
                }
            }
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl SessionLog {
    /// What's loaded of the log and what's on its way, for a test whose wait for a row timed out to say.
    pub fn loading(&self) -> String {
        let (last, received) = (self.last.row(), self.received.row());
        let (pages, in_flight, failed, stale) = (&self.pages, &self.in_flight, &self.failed, &self.stale);
        format!(
            "the log has {} rows, the last at row {last:?}, and the subscription has delivered through row {received:?}; \
             rows {:?} are loaded, from pages {pages:?}, with pages {in_flight:?} on their way, {failed:?} failed and \
             {stale:?} stale; the view looks at rows {:?} and couldn't draw row {:?}",
            self.row_count(),
            self.loaded_rows(),
            self.focus,
            self.unfetched,
        )
    }
}

/// The page the event at `position` is in, counting from 0, a page to each [`PAGE_EVENTS`] rows. 0, the end of a log
/// that reaches nowhere, is in the first.
fn page_of(position: Position) -> usize {
    position.row().unwrap_or(0) / PAGE_EVENTS
}

pub(super) fn page_range(page: usize) -> Range<Position> {
    Position::of_rows(page * PAGE_EVENTS..(page + 1) * PAGE_EVENTS)
}

/// The position a subscription for a log that reaches `last` starts after: the one before the page before the newest.
/// Every page after it comes live and whole, with an open approval, which is usually near the end, and the events up to
/// it come by the page. It's 0 when the subscription starts from the first.
pub(super) fn before_live(last: Position) -> Position {
    let first = page_of(last).saturating_sub(1) * PAGE_EVENTS;
    first.checked_sub(1).map_or(Position::default(), Position::of_row)
}
