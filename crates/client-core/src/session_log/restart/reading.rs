use protocol::model::{Place, Seq};

use super::super::pages::page_range;
use super::super::{Position, SessionLog, before_live};

impl SessionLog {
    /// Learns how far the events from one agent or subagent go, which a log by place starts out not knowing.
    pub(crate) fn reaches(&mut self, last: Place) {
        let last = Position::of_place(last);
        // Only now can it start where [`SessionLog::new`] starts one that knows: live from near the end, and looking at
        // the last row, so it fetches nothing until a view asks.
        if self.received == Position::default() {
            self.received = before_live(last);
            self.focus = last.row().unwrap_or(0);
        }
        self.last = self.last.max(last);
        self.touched(0..self.row_count() + 1);
    }

    /// Becomes the whole log, which reaches `last_seq`, for a hub that can't read one agent's events apart.
    pub(crate) fn read_whole(&mut self, last_seq: Seq) {
        self.restart(Self::new(last_seq));
    }

    /// Becomes one agent's or subagent's events again, which reach nowhere until [`SessionLog::reaches`] says, for a
    /// hub that can read them apart after one that couldn't, such as a newer one after a restart.
    pub(crate) fn read_by_place(&mut self) {
        self.restart(Self::by_place(Place::default()));
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

    /// Starts again as `fresh`, keeping the prompts on their way, those recorded that it knows of and where each was
    /// settled, where a view is looking, what it has yet to hear of, and which requests wait, which are fetched again,
    /// and telling it every row changed, the prompts' among them. Events added go on counting, since a view may have
    /// kept the count.
    fn restart(&mut self, fresh: Self) {
        let rows = self.row_count();
        let old = std::mem::replace(self, fresh);
        self.pending = old.pending;
        self.focus = old.focus;
        self.changes = old.changes;
        self.outline = old.outline;
        self.settled = old.settled;
        self.apart = old.apart.again();
        self.outline.moved(self.by);
        self.events_added = old.events_added + 1;
        self.touched(0..rows.max(self.row_count()) + 1);
    }
}
