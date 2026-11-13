//! Which stretches of a session's log a client fetches, keeps and lets go of, a page of [`PAGE_EVENTS`] at a time,
//! around the row a view wants and at the end, which a subscription delivers, and what's asked for again after a fetch
//! fails.

use std::ops::Range;

use protocol::event::{EventBody, SessionEvent};
use protocol::model::{SettledAt, seq_index};

use super::{MAX_LOADED_EVENTS, SessionLog};

pub(super) const PAGE_EVENTS: u64 = 250;

impl SessionLog {
    /// Notes the row a view wants, which decides what to fetch and what stays loaded.
    pub(crate) fn focus_on(&mut self, row: usize) {
        self.focus = row;
    }

    /// Pages to fetch for the row a view wants, nearest first, marking them as in flight. A page the
    /// subscription has yet to deliver isn't one of them.
    pub(crate) fn pages_to_fetch(&mut self) -> Vec<Range<u64>> {
        if self.last == 0 {
            return Vec::new();
        }
        let focus = self.focus.min(seq_index(self.last) - 1) as u64 + 1;
        let center = page_of(focus);
        let last = page_of(self.last);
        let mut wanted = vec![center];
        if center > 0 {
            wanted.push(center - 1);
        }
        if center < last {
            wanted.push(center + 1);
        }
        let mut out = Vec::new();
        for page in wanted {
            let fetchable = page_range(page).start <= self.received && !self.failed.contains(&page);
            if fetchable && !self.pages.contains(&page) && self.in_flight.insert(page) {
                out.push(page_range(page));
            }
        }
        out
    }

    /// Puts a fetched range in place.
    pub(crate) fn install_page(&mut self, start: u64, events: Vec<SessionEvent>) {
        let page = page_of(start);
        self.in_flight.remove(&page);
        self.pages.insert(page);
        let range = page_range(page);
        let (mut settles, mut prompts) = (Vec::new(), Vec::new());
        for event in events {
            self.last = self.last.max(self.position(&event));
            self.outline.saw(&event);
            settles.extend(SettledAt::of(&event));
            prompts.extend(matches!(event.body, EventBody::UserPrompt { .. }).then_some(event.seq));
            self.insert(event);
        }
        self.events_added += 1;
        self.evict();
        self.touched(seq_index(range.start - 1)..seq_index((range.end - 1).min(self.last)));
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
    pub(super) fn keep_live(&mut self, position: u64, event: SessionEvent) {
        if page_of(position) + 1 >= page_of(self.last) {
            self.pages.insert(page_of(position));
            self.insert(event);
            self.events_added += 1;
            self.evict();
        }
    }

    /// Drops whole pages furthest from where a view is looking, keeping the tail for live updates.
    fn evict(&mut self) {
        let center = page_of(self.focus as u64 + 1);
        let tail = page_of(self.last);
        while self.events.len() > MAX_LOADED_EVENTS {
            let far = self
                .pages
                .iter()
                .copied()
                .filter(|page| page.abs_diff(center) > 1 && page.abs_diff(tail) > 1)
                .max_by_key(|page| page.abs_diff(center));
            let Some(far) = far else { return };
            self.pages.remove(&far);
            let range = page_range(far);
            let doomed: Vec<u64> = self.events.range(range).map(|(position, _)| *position).collect();
            for position in doomed {
                if let Some(event) = self.events.remove(&position) {
                    self.positions.remove(&event.seq);
                    // A request that waits stays, so what waits is still asked.
                    self.apart.keep_if_waiting(event);
                }
            }
        }
    }
}

fn page_of(position: u64) -> u64 {
    (position.max(1) - 1) / PAGE_EVENTS
}

/// Where a subscription starts for a log that reaches `last`: the start of the page before the newest,
/// so every page it delivers arrives whole, and an open approval, which is usually near the end, comes too.
pub(super) fn first_live(last: u64) -> u64 {
    page_range(page_of(last).saturating_sub(1)).start
}

fn page_range(page: u64) -> Range<u64> {
    page * PAGE_EVENTS + 1..(page + 1) * PAGE_EVENTS + 1
}
