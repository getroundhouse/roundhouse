//! What a client draws of a workspace's lines added and removed against its base, from the hub's observation of them,
//! so every client draws the same: nothing while they're not known, a count of a thousand or more shortened, and lines
//! last read some time ago told apart from lines kept current. `docs/spec/sidebar.md` and `docs/spec/home.md` have the
//! rules.

use protocol::model::{LineCounts, Observed, WorkspaceStatus};

/// A workspace's lines added and removed, as a client draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawn {
    pub added: usize,
    pub removed: usize,
    /// They're as the hub last read them, at some time, rather than kept current, so they're drawn fainter, and no
    /// client shows an old reading as current.
    pub as_last_read: bool,
}

impl Drawn {
    /// What they say to assistive apps, which read no colour: `12 lines added and 3 removed`, leaving out a side that's
    /// none, as the lines drawn do, and saying when they're as last read.
    pub fn said(&self) -> String {
        let lines = |count| if count == 1 { "line" } else { "lines" };
        let said = match (self.added, self.removed) {
            (added, 0) => format!("{added} {} added", lines(added)),
            (0, removed) => format!("{removed} {} removed", lines(removed)),
            (added, removed) => format!("{added} {} added and {removed} removed", lines(added)),
        };
        if self.as_last_read { format!("{said}, as last read") } else { said }
    }
}

/// What a workspace with `status` draws of `lines`. Nothing while they're not known, or for a workspace with no
/// changes, which has no side to draw, since each is left out when it's none. Nothing either for a workspace that isn't
/// ready to work in: one being made has no worktree to change yet, one being archived or brought back is on its way,
/// and an archived one's worktree is gone, so what was read of it is of nothing that's there.
pub fn drawn(status: WorkspaceStatus, lines: &Observed<LineCounts>) -> Option<Drawn> {
    let (counts, as_last_read) = match lines {
        Observed::Current { value } => (value, false),
        Observed::AsOf { value, .. } => (value, true),
        Observed::NotKnown | Observed::Unknown => return None,
    };
    let ready = match status {
        WorkspaceStatus::Ready => true,
        WorkspaceStatus::Creating
        | WorkspaceStatus::Archiving
        | WorkspaceStatus::Archived
        | WorkspaceStatus::Restoring
        | WorkspaceStatus::Unknown => false,
    };
    let drawn = Drawn { added: counted(counts.added), removed: counted(counts.removed), as_last_read };
    (ready && (drawn.added > 0 || drawn.removed > 0)).then_some(drawn)
}

/// A count of lines where there's room only for a few figures, such as `37k`: whole below a thousand, then in
/// thousands, and from a million in millions, with a tenth below ten of either, such as `1.2k`. Cut rather than
/// rounded, so a count never reads as more than it is, and `999,999` reads as `999k` rather than `1000k`.
pub fn shortened(count: usize) -> String {
    for (unit, suffix) in [(1_000_000, "M"), (1_000, "k")] {
        if count < unit {
            continue;
        }
        let whole = count / unit;
        let tenths = count / (unit / 10) % 10;
        return if whole < 10 && tenths > 0 { format!("{whole}.{tenths}{suffix}") } else { format!("{whole}{suffix}") };
    }
    count.to_string()
}

/// A count the hub sends, of lines or of files, as a client counts them. `usize` is 64 bits on every platform a client
/// runs on, so it never saturates.
pub fn counted(count: u64) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use protocol::model::{LineCounts, Observed, WorkspaceStatus};

    use super::{Drawn, drawn, shortened};

    #[test]
    fn a_count_is_whole_below_a_thousand_and_cut_to_a_tenth_below_ten_of_a_unit() {
        let cases = [
            (0, "0"),
            (999, "999"),
            (1_000, "1k"),
            (1_099, "1k"),
            (1_250, "1.2k"),
            (9_999, "9.9k"),
            (10_000, "10k"),
            (37_412, "37k"),
            (999_999, "999k"),
            (1_000_000, "1M"),
            (1_480_000, "1.4M"),
            (37_000_000, "37M"),
            (4_000_000_000, "4000M"),
        ];
        for (count, written) in cases {
            assert_eq!(shortened(count), written, "{count}");
        }
    }

    #[test]
    fn lines_say_what_they_are_in_words() {
        let said = |added, removed, as_last_read| Drawn { added, removed, as_last_read }.said();

        assert_eq!(said(12, 3, false), "12 lines added and 3 removed");
        assert_eq!(said(1, 1, false), "1 line added and 1 removed");
        assert_eq!(said(40, 0, false), "40 lines added");
        assert_eq!(said(0, 1, false), "1 line removed");
        assert_eq!(said(37_412, 0, true), "37412 lines added, as last read");
    }

    #[test]
    fn lines_are_drawn_only_once_known_in_a_workspace_that_is_ready_and_has_changes() {
        let counts = LineCounts { added: 12, removed: 3 };
        let current = Observed::Current { value: counts };
        let as_of = Observed::AsOf { value: counts, as_of_ms: 1_000 };
        let drawn_as = |as_last_read| Some(Drawn { added: 12, removed: 3, as_last_read });
        let none = Observed::Current { value: LineCounts::default() };
        let one_sided = Observed::Current { value: LineCounts { added: 0, removed: 7 } };

        assert_eq!(drawn(WorkspaceStatus::Ready, &current), drawn_as(false));
        assert_eq!(drawn(WorkspaceStatus::Ready, &as_of), drawn_as(true), "fainter, as last read");
        assert_eq!(drawn(WorkspaceStatus::Ready, &Observed::NotKnown), None);
        assert_eq!(drawn(WorkspaceStatus::Ready, &Observed::Unknown), None, "as not known");
        assert_eq!(drawn(WorkspaceStatus::Ready, &none), None, "no side to draw");
        let only_removed = Some(Drawn { added: 0, removed: 7, as_last_read: false });
        assert_eq!(drawn(WorkspaceStatus::Ready, &one_sided), only_removed);
        for status in [
            WorkspaceStatus::Creating,
            WorkspaceStatus::Archiving,
            WorkspaceStatus::Archived,
            WorkspaceStatus::Restoring,
            WorkspaceStatus::Unknown,
        ] {
            assert_eq!(drawn(status, &current), None, "{status:?}");
        }
    }
}
