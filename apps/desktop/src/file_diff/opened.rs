//! The lines git left out of a file's diff that have been opened, by their numbers on the old side, which stay the same
//! for as long as the diff is taken from the same commit, however the file changes after it. So what's opened stays
//! open when the diff is read again, and when another file has shown in between.

use std::ops::Range;

use protocol::model::CommitId;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Opened {
    /// The commit the numbers are on.
    commit: Option<CommitId>,
    /// In order, and apart from each other.
    lines: Vec<Range<u32>>,
}

impl Opened {
    /// Opens `lines`, numbered on `commit`, forgetting what was opened on another.
    pub(super) fn open(&mut self, commit: Option<&CommitId>, lines: Range<u32>) {
        if self.commit.as_ref() != commit {
            *self = Self { commit: commit.cloned(), lines: Vec::new() };
        }
        if lines.is_empty() {
            return;
        }
        let first = self.lines.partition_point(|open| open.end < lines.start);
        let last = self.lines.partition_point(|open| open.start <= lines.end);
        let joined = self.lines[first..last]
            .iter()
            .fold(lines, |joined, open| joined.start.min(open.start)..joined.end.max(open.end));
        self.lines.splice(first..last, [joined]);
    }

    /// What's open of `lines`, numbered on `commit`, in order.
    pub(super) fn within(&self, commit: Option<&CommitId>, lines: Range<u32>) -> Vec<Range<u32>> {
        if self.commit.as_ref() != commit {
            return Vec::new();
        }
        self.lines
            .iter()
            .map(|open| open.start.max(lines.start)..open.end.min(lines.end))
            .filter(|open| !open.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_touches_what_was_opened_joins_it() {
        let commit = CommitId::from("c1");
        let mut opened = Opened::default();

        opened.open(Some(&commit), 10..20);
        opened.open(Some(&commit), 30..40);
        opened.open(Some(&commit), 20..25);

        assert_eq!(opened.within(Some(&commit), 0..100), [10..25, 30..40]);
        opened.open(Some(&commit), 24..31);
        assert_eq!(opened.within(Some(&commit), 15..35), vec![15..35]);
    }

    #[test]
    fn lines_opened_on_another_commit_are_forgotten() {
        let (first, second) = (CommitId::from("c1"), CommitId::from("c2"));
        let mut opened = Opened::default();
        opened.open(Some(&first), 10..20);

        assert!(opened.within(Some(&second), 0..100).is_empty());
        opened.open(Some(&second), 50..60);
        assert_eq!(opened.within(Some(&second), 0..100), vec![50..60]);
        assert!(opened.within(Some(&first), 0..100).is_empty());
    }
}
