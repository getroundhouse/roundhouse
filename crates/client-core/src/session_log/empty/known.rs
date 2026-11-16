//! Runs of fetched `AgentOther` rows. They never draw anything or supply another row's content, so keeping their
//! positions is enough. Their bodies and raw lines remain in the hub. A fold hiding a row is not such a run.

use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Clone, Debug, Default)]
pub(in super::super) struct Empty(BTreeMap<usize, usize>);

impl Empty {
    pub(in super::super) fn contains(&self, row: usize) -> bool {
        self.0.range(..=row).next_back().is_some_and(|(_, end)| row < *end)
    }

    pub(in super::super) fn insert(&mut self, row: usize) {
        let mut run = row..row + 1;
        if let Some((&start, &end)) = self.0.range(..=row).next_back() {
            if end > row {
                return;
            }
            if end == row {
                run.start = start;
                self.0.remove(&start);
            }
        }
        if let Some(end) = self.0.remove(&run.end) {
            run.end = end;
        }
        self.0.insert(run.start, run.end);
    }

    /// A migration may have made a formerly empty row visible.
    pub(in super::super) fn remove(&mut self, row: usize) {
        let Some((&start, &end)) = self.0.range(..=row).next_back().filter(|(_, end)| row < **end) else { return };
        self.0.remove(&start);
        if start < row {
            self.0.insert(start, row);
        }
        if row + 1 < end {
            self.0.insert(row + 1, end);
        }
    }

    pub(in super::super) fn ranges(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.0.iter().map(|(start, end)| *start..*end)
    }

    pub(in super::super) fn count(&self, rows: Range<usize>) -> usize {
        if rows.is_empty() {
            return 0;
        }
        let before =
            self.0.range(..rows.start).next_back().map_or(0, |(_, end)| end.min(&rows.end).saturating_sub(rows.start));
        before + self.0.range(rows.clone()).map(|(start, end)| end.min(&rows.end) - start).sum::<usize>()
    }
}
