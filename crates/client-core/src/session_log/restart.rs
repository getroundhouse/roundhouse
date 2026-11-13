//! A log that learns how far it goes, or starts again following other events, when a hub reads them differently from
//! the one before.

use protocol::model::{Seq, seq_index};

use super::{SessionLog, first_live};

impl SessionLog {
    /// Learns how far the events from one agent or subagent go, which a log by place starts out not knowing.
    pub(crate) fn reaches(&mut self, last: u64) {
        // Only now can it start where [`SessionLog::new`] starts one that knows: live from near the end, and looking at
        // the last row, so it fetches nothing until a view asks.
        if self.received == 0 {
            self.received = first_live(last) - 1;
            self.focus = seq_index(last).saturating_sub(1);
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
        self.restart(Self::by_place(0));
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
