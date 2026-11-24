use protocol::model::Place;

use super::super::pages::page_range;
use super::super::{Position, SessionLog, before_live};

impl SessionLog {
    /// Learns how far the events from one agent or subagent go, which a log starts out not knowing until the hub says.
    pub(crate) fn reaches(&mut self, last: Place) {
        let last = Position::of_place(last);
        // Only now can it start where [`SessionLog::new`] starts one that knows: live from near the end, and looking at
        // the last row, so it fetches nothing until a view asks.
        if self.received == Position::default() {
            self.received = before_live(last);
            let row = last.row().unwrap_or(0);
            self.focus = row..row + 1;
            self.unfetched = None;
        }
        self.last = self.last.max(last);
        self.touched(0..self.row_count() + 1);
    }

    /// Reads its events again where they are, for a hub of another build, whose migration can have rewritten them
    /// without moving any. Each stays drawn until it comes again, so a chat stays on screen through an update: the
    /// newest pages come live, as when the log was opened, every other page it holds is fetched again as a view looks
    /// at it, and so is each request waiting that was fetched on its own. The hub is asked for the prompts and
    /// compactions again too.
    pub(crate) fn read_again(&mut self) {
        let received = before_live(self.last);
        self.received = received;
        self.stale = self.pages.iter().copied().filter(|page| page_range(*page).start <= received).collect();
        self.apart.read_again();
        self.outline.list_again();
    }
}
