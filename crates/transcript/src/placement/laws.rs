//! What has to hold however the log changes, as it streams, as pages load and as the log lets go of them, and
//! whichever calls are opened or closed.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use client_core::session_log::SessionLog;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::Seq;

use super::tests::drawn;
use super::{LogRow, Placement, Position};
use crate::fixtures::{calls, calls_to, event, text, within};
use crate::folds::Folds;
use crate::item_key::ItemKey;

/// What an event in a made-up log is.
#[derive(Clone, Debug)]
enum Kind {
    /// The agent's own words.
    Own,
    /// The agent starting a subagent.
    Starts,
    /// Work from a subagent, naming the call this many rows back, which may not be one that starts a subagent.
    Work(usize),
    /// Work that starts a subagent of its own.
    WorkStarting(usize),
    /// The agent starting two subagents at once.
    StartsTwo,
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![
        3 => Just(Kind::Own),
        2 => Just(Kind::Starts),
        5 => (1usize..8).prop_map(Kind::Work),
        1 => (1usize..8).prop_map(Kind::WorkStarting),
        1 => Just(Kind::StartsTwo),
    ]
}

/// Work names the call it's from by the row it's in, so it can name either of two calls in a row by the row's parity.
fn body(row: usize, kind: &Kind) -> EventBody {
    let call = |back: usize| {
        let at = row.saturating_sub(back);
        if back.is_multiple_of(2) { format!("call-{at}-second") } else { format!("call-{at}") }
    };
    match kind {
        Kind::Own => text("own"),
        Kind::Starts => calls_to("Agent", &[&format!("call-{row}")]),
        Kind::Work(back) => within(&call(*back), calls(&["work"])),
        Kind::WorkStarting(back) => within(&call(*back), calls_to("Agent", &[&format!("call-{row}")])),
        Kind::StartsTwo => calls_to("Agent", &[&format!("call-{row}"), &format!("call-{row}-second")]),
    }
}

fn seq(row: usize) -> Seq {
    Seq::try_from(row + 1).unwrap()
}

/// The first `rows` events, holding only those `loaded` says. The last is always held, so the log knows how long it is.
fn held(kinds: &[Kind], rows: usize, loaded: &[bool]) -> SessionLog {
    let mut log = SessionLog::new(0);
    for (row, kind) in kinds.iter().enumerate().take(rows) {
        if loaded[row] || row + 1 == rows {
            log.apply_event(event(seq(row), body(row, kind)));
        }
    }
    log
}

/// Opens or closes each call in the log as `open` says, by its row, as clicking it would. A row's second call, if it
/// has one, is the opposite of its first.
fn choose(folds: &mut Folds, kinds: &[Kind], open: &[bool]) {
    for (row, kind) in kinds.iter().enumerate() {
        let calls = match kind {
            Kind::Own | Kind::Work(_) => 0,
            Kind::Starts | Kind::WorkStarting(_) => 1,
            Kind::StartsTwo => 2,
        };
        for position in 0..calls {
            let key = ItemKey::of_block(seq(row), &body(row, kind), position);
            folds.choose(key, open[row] == (position == 0));
        }
    }
}

/// The calls a row is drawn under, innermost first, and whether they're all open. Two rows drawn under the same
/// calls keep their order.
fn under(placement: &Placement, row: usize) -> (Vec<String>, bool) {
    let (mut calls, mut open) = (Vec::new(), true);
    let mut at = LogRow(row);
    while let Some(id) = placement.within(at) {
        let call = placement.call(id).unwrap();
        open &= call.open;
        at = call.row;
        calls.push(id.to_string());
    }
    (calls, open)
}

/// What of a row's look depends on other rows: whether each call in it has work drawn after it, and which calls' rules
/// it ends, and whether those subagents wrote anything.
fn look(placement: &Placement, row: usize) -> Vec<(String, bool, bool, bool)> {
    let row = LogRow(row);
    let mut look: Vec<_> = placement
        .calls
        .iter()
        .filter(|(_, call)| call.row == row || call.work.last == Some(row))
        .map(|(id, call)| {
            (id.to_string(), call.row == row && call.work.last.is_some(), call.work.last == Some(row), call.work.said)
        })
        .collect();
    look.sort();
    look
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// Making the splices it returns turns the list as it was into the list as it is, and the only rows they take
    /// out are ones drawn somewhere else now, or not at all, so the rest keep what the list measured and where it's
    /// scrolled to.
    #[test]
    fn its_splices_move_only_the_rows_that_moved(
        kinds in vec(kind(), 1..60),
        (loaded_before, loaded_after) in (vec(any::<bool>(), 60), vec(any::<bool>(), 60)),
        (open_before, open_after) in (vec(any::<bool>(), 60), vec(any::<bool>(), 60)),
        grows in 0usize..60,
    ) {
        let rows_before = kinds.len().saturating_sub(grows).max(1);
        let mut folds = Folds::default();
        choose(&mut folds, &kinds, &open_before);
        let old = Placement::of(&held(&kinds, rows_before, &loaded_before), &folds);
        let mut placement = Placement::of(&held(&kinds, rows_before, &loaded_before), &folds);
        let log = held(&kinds, kinds.len(), &loaded_after);
        choose(&mut folds, &kinds, &open_after);

        let update = placement.follow(&log, &folds);

        let mut list: Vec<Option<usize>> = drawn(&old).into_iter().map(Some).collect();
        for splice in update.splices {
            list.splice(splice.at.0..splice.at.0 + splice.removed, std::iter::repeat_n(None, splice.added));
        }
        let new = Placement::of(&log, &folds);
        prop_assert_eq!(drawn(&placement), drawn(&new));
        prop_assert_eq!(list.len(), new.len());
        let was_drawn = drawn(&old);
        for (position, kept) in list.iter().enumerate() {
            let row = new.row_at(Position(position)).unwrap().0;
            let stayed = was_drawn.contains(&row) && under(&old, row).0 == under(&new, row).0;
            prop_assert_eq!(*kept, stayed.then_some(row), "row {} at {}", row, position);
            // A row the list keeps, whose look changed because of others, is measured again.
            let restyled = update.restyled.iter().any(|run| run.contains(&Position(position)));
            if kept.is_some() && look(&old, row) != look(&new, row) {
                prop_assert!(restyled, "row {} at {} looks different, but isn't measured again", row, position);
            }
        }
        prop_assert!(update.restyled.iter().all(|run| run.end.0 <= new.len()), "it measures only rows the list has");
    }

    /// Every row the log has is drawn exactly once, where it says it is, unless it's work under a closed call.
    #[test]
    fn every_row_not_under_a_closed_call_is_drawn_once(
        kinds in vec(kind(), 1..60),
        loaded in vec(any::<bool>(), 60),
        open in vec(any::<bool>(), 60),
    ) {
        let mut folds = Folds::default();
        choose(&mut folds, &kinds, &open);

        let placement = Placement::of(&held(&kinds, kinds.len(), &loaded), &folds);

        let mut rows = drawn(&placement);
        for (position, &row) in rows.iter().enumerate() {
            prop_assert_eq!(placement.position_of(LogRow(row)), Some(Position(position)));
        }
        rows.sort_unstable();
        let shown: Vec<usize> = (0..kinds.len()).filter(|&row| under(&placement, row).1).collect();
        prop_assert_eq!(rows, shown);
    }
}

#[test]
fn a_made_up_log_has_the_events_it_was_made_with() {
    let kinds = [Kind::Starts, Kind::Work(1), Kind::Own];

    let log = held(&kinds, 3, &[true, false, true]);

    let held: Vec<&SessionEvent> = log.loaded().collect();
    assert_eq!(held.iter().map(|event| event.seq).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(log.row_count(), 3);
}
