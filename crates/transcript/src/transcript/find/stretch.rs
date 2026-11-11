//! Counting the matches in rows the log doesn't hold, from the stretches of the chat client-core's search reads around
//! each event the hub named. Each is put in a log of its own, by place as the chat's is, counted the way the log's own
//! rows are, and let go, so the transcript never holds more than it would anyway.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use client_core::Stretch;
use client_core::session_log::SessionLog;
use gpui_kit::{AppContext as _, Context, Task};
use protocol::event::SessionEvent;
use protocol::find::Needle;
use protocol::model::{AgentKind, Subagent, ToolUseId};

use super::is_loaded;
use crate::rows::{found_in_row, rows_to_count};
use crate::transcript::{AtWork, Model, Transcript};

/// The subagents' records, by the call that started each, copied to go to the thread that counts.
pub(super) type Records = HashMap<ToolUseId, Subagent>;

impl Transcript {
    /// Counts the rows that draw the events `stretch` was read for, on another thread, then takes the counts of those
    /// the log doesn't hold, and goes on to the next stretch. Those the log holds are counted as they're drawn, from the
    /// log.
    pub(super) fn count_stretch(
        stretch: Stretch,
        needle: Needle,
        records: Records,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let Stretch { events, holding } = stretch;
            let (holding, counted) = cx
                .background_spawn(async move {
                    let counted = counted_in(events, &holding, &needle, records);
                    (holding, counted)
                })
                .await;
            let _ = this.update(cx, |t, cx| t.counted_stretch(holding, counted, cx));
        })
    }

    fn counted_stretch(&mut self, holding: Vec<u64>, counted: Vec<(usize, usize)>, cx: &mut Context<Self>) {
        let counted: Vec<(usize, usize)> =
            self.session.read(|held| counted.into_iter().filter(|(row, _)| !is_loaded(held, *row)).collect());
        let Some(query) = self.query_mut() else { return };
        query.counting = None;
        query.candidates.extend(holding);
        for (row, count) in counted {
            query.counts.set(row, count);
        }
        self.find_news(cx);
    }
}

/// What the rows that draw each of the events at the places `holding` count to, among `events`, a stretch of the chat
/// read for them, by place, with the subagents' `records`, which their calls draw.
pub(super) fn counted_in(
    events: Vec<SessionEvent>,
    holding: &[u64],
    needle: &Needle,
    records: Records,
) -> Vec<(usize, usize)> {
    let mut log = SessionLog::by_place(0);
    for event in events {
        log.apply_event(event);
    }
    // Its own, since what it remembers of where calls are is for the transcript's log, and the agent it names only
    // says how a turn's card reads, which isn't found.
    let mut m = Model::with(AgentKind::Claude, AtWork::default());
    m.subagents = records.into_iter().map(|(call, record)| (call, Rc::new(record))).collect();
    let rows: BTreeSet<usize> = holding.iter().flat_map(|place| rows_to_count(&log, &m, *place)).collect();
    rows.into_iter()
        .map(|row| (row, found_in_row(&log, &m, needle, row).iter().map(|found| found.matches).sum()))
        .collect()
}
