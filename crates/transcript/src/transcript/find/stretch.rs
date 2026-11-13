//! Counting the matches in rows the log doesn't hold, from the stretches of the chat client-core's search reads around
//! each event the hub named. Each is put in a log of its own, by place as the chat's is, counted the way the log's own
//! rows are, and let go, so the transcript never holds more than it would anyway.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use client_core::Stretch;
use client_core::session_log::{Position, SessionLog, Settled};
use gpui_kit::{AppContext as _, Context, Task};
use protocol::event::SessionEvent;
use protocol::find::Needle;
use protocol::model::{Place, Subagent, ToolUseId};

use super::is_loaded;
use crate::rows::{found_in_row, rows_to_count};
use crate::transcript::{AtWork, Model, Transcript};

/// The subagents' records, by the call that started each, copied to go to the thread that counts.
pub(super) type Records = HashMap<ToolUseId, Arc<Subagent>>;

impl Transcript {
    /// Counts the rows that draw the events `stretch` was read for, on another thread, then takes the counts of those
    /// the log doesn't hold, and goes on to the next stretch. Those the log holds are counted as they're drawn, from the
    /// log. `known` is what the chat's log knows, so the stretch draws what the chat does.
    pub(super) fn count_stretch(stretch: Stretch, needle: Needle, known: Known, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let Stretch { events, holding } = stretch;
            let (holding, counted) = cx
                .background_spawn(async move {
                    let counted = counted_in(events, &holding, &needle, known);
                    (holding, counted)
                })
                .await;
            let _ = this.update(cx, |t, cx| t.counted_stretch(holding, counted, cx));
        })
    }

    fn counted_stretch(&mut self, holding: Vec<Position>, counted: Vec<(usize, usize)>, cx: &mut Context<Self>) {
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

/// What drawing a stretch of the chat needs from the rest of it, copied to go to the thread that counts: the subagents'
/// records, which their calls draw, and where each prompt was settled, which is where it's drawn.
#[derive(Default)]
pub(super) struct Known {
    pub(super) records: Records,
    pub(super) settled: Settled,
}

/// What the rows that draw each of the events at `holding` count to, among `events`, a stretch of the chat read for
/// them, by place, with what's `known` of the rest. A prompt settled past the end of the stretch is drawn in a row it
/// doesn't hold, so it counts once that row is loaded.
pub(super) fn counted_in(
    events: Vec<SessionEvent>,
    holding: &[Position],
    needle: &Needle,
    known: Known,
) -> Vec<(usize, usize)> {
    let Known { records, settled } = known;
    let mut log = SessionLog::by_place(Place::default()).knowing(settled);
    for event in events {
        log.apply_event(event);
    }
    // Its own, since what it remembers of where calls are is for the transcript's log, and the agent it names only
    // says how a turn's card reads, which isn't found.
    let mut m = Model::with(AtWork::default());
    m.subagents = records;
    let rows: BTreeSet<usize> = holding.iter().flat_map(|position| rows_to_count(&log, &m, *position)).collect();
    rows.into_iter()
        .map(|row| (row, found_in_row(&log, &m, needle, row).iter().map(|found| found.matches).sum()))
        .collect()
}
