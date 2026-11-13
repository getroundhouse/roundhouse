use super::super::opened::Opened;
use crate::patch::{FilePatch, Hunk, LineKind, Unchanged};
use protocol::model::{CommentId, CommitId, DiffSide};
use std::collections::HashMap;
use std::ops::Range;

/// How many lines a fold's arrow opens at a time.
pub(in super::super) const STEP: u32 = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Row {
    /// A line in one column: one both sides have, or one only the old or the new side has.
    Line {
        kind: LineKind,
        old: Option<u32>,
        new: Option<u32>,
    },
    /// The old side's line beside the new side's, either of which can be missing, or one line both sides have.
    Pair {
        old: Option<u32>,
        new: Option<u32>,
        same: bool,
    },
    Fold(Fold),
    Comment(CommentId),
    /// The comment being written.
    Draft,
}

/// Lines git left out that aren't open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct Fold {
    /// By their numbers on the old side.
    pub(in super::super) old: Range<u32>,
    /// Where they start on the new side.
    pub(in super::super) new_start: u32,
    /// Whether they're the file's first lines, so there's no change above them to open them down from.
    pub(in super::super) top: bool,
    /// Whether they're its last.
    pub(in super::super) end: bool,
    /// Whether they can be opened, which they can once the file as it was has been read.
    pub(in super::super) opens: bool,
}

/// What's on a line below it: a comment, or the one being written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct Note {
    pub(in super::super) side: DiffSide,
    pub(in super::super) line: u32,
    /// [`Row::Comment`] or [`Row::Draft`].
    pub(in super::super) row: Row,
}

/// What the rows are made from.
pub(in super::super) struct Shape<'a> {
    pub(in super::super) file: &'a FilePatch,
    pub(in super::super) old: Old<'a>,
    pub(in super::super) opened: &'a Opened,
    /// The commit the diff is taken from, which what's opened is numbered on.
    pub(in super::super) commit: Option<&'a CommitId>,
    pub(in super::super) split: bool,
    /// Every line shows, with nothing folded.
    pub(in super::super) everything: bool,
    pub(in super::super) notes: &'a [Note],
}

/// What's known of the old side's lines, so what git left out can show.
#[derive(Clone, Copy, Debug)]
pub(in super::super) enum Old<'a> {
    /// Only the hunks'.
    Hunks,
    /// Every one of them, which number this many.
    Whole(u32),
    /// Those `known`, by number, in order, of `lines`. The others stay folded until they've been read.
    InPart { lines: u32, known: &'a [Range<u32>] },
}

impl Old<'_> {
    pub(super) fn lines(self) -> Option<u32> {
        match self {
            Self::Hunks => None,
            Self::Whole(lines) | Self::InPart { lines, .. } => Some(lines),
        }
    }
}

/// Where row `row` of `was` is in `now`: the same row, or for one that's gone, the first at or after its line, on the old
/// side if it has one, since a line's number there stays the same however the file changes after the diff's commit.
pub(in super::super) fn same_row(was: &[Row], now: &[Row], row: usize) -> Option<usize> {
    let row = was.get(row)?;
    if let Some(same) = now.iter().position(|other| other == row) {
        return Some(same);
    }
    let lines = |row: &Row| match row {
        Row::Line { old, new, .. } | Row::Pair { old, new, .. } => (*old, *new),
        Row::Fold(fold) => (Some(fold.old.start), Some(fold.new_start)),
        Row::Comment(_) | Row::Draft => (None, None),
    };
    match lines(row) {
        (Some(old), _) => now.iter().position(|other| lines(other).0.is_some_and(|line| line >= old)),
        (None, Some(new)) => now.iter().position(|other| lines(other).1.is_some_and(|line| line >= new)),
        (None, None) => None,
    }
}

/// The rows, and the lines of the old side they'd show that haven't been read, in order and apart from each other.
pub(in super::super) struct Laid {
    pub(in super::super) rows: Vec<Row>,
    pub(in super::super) missing: Vec<Range<u32>>,
}

pub(in super::super) fn rows(shape: &Shape<'_>) -> Laid {
    let mut rows = Rows::new(shape);
    let unchanged = shape.file.unchanged(shape.old.lines().map_or(u32::MAX, |length| length.saturating_add(1)));
    for (gap, hunk) in unchanged.iter().cloned().zip(shape.file.hunks()) {
        rows.gap(&gap);
        rows.hunk(hunk);
    }
    if let (Some(_), Some(last)) = (shape.old.lines(), unchanged.last()) {
        rows.gap(last);
    }
    rows.finish()
}

/// The rows as they're laid out, and which notes have found their line.
struct Rows<'a> {
    pub(super) shape: &'a Shape<'a>,
    pub(super) laid_out: Vec<Row>,
    pub(super) missing: Vec<Range<u32>>,
    /// Each note's index, by its side, as whether it's the old one, and its line.
    pub(super) notes: HashMap<(bool, u32), Vec<usize>>,
    pub(super) placed: Vec<bool>,
}

fn is_old(side: DiffSide) -> bool {
    match side {
        DiffSide::Old => true,
        DiffSide::New | DiffSide::Unknown => false,
    }
}

impl<'a> Rows<'a> {
    pub(super) fn new(shape: &'a Shape<'a>) -> Self {
        let mut notes: HashMap<(bool, u32), Vec<usize>> = HashMap::new();
        for (ix, note) in shape.notes.iter().enumerate() {
            notes.entry((is_old(note.side), note.line)).or_default().push(ix);
        }
        Self { shape, laid_out: Vec::new(), missing: Vec::new(), notes, placed: vec![false; shape.notes.len()] }
    }

    /// The notes on `line` of a side, after whatever's drawn of it.
    pub(super) fn notes_on(&mut self, old: bool, line: u32) {
        for &ix in self.notes.get(&(old, line)).map_or(&[][..], Vec::as_slice) {
            if !std::mem::replace(&mut self.placed[ix], true) {
                self.laid_out.push(self.shape.notes[ix].row.clone());
            }
        }
    }

    /// A line both sides have, and its notes.
    pub(super) fn same(&mut self, old: u32, new: u32) {
        let row = if self.shape.split {
            Row::Pair { old: Some(old), new: Some(new), same: true }
        } else {
            Row::Line { kind: LineKind::Same, old: Some(old), new: Some(new) }
        };
        self.laid_out.push(row);
        self.notes_on(true, old);
        self.notes_on(false, new);
    }

    /// Lines between hunks, or before the first or after the last. They show where they're open, where
    /// there's a note on one, or everywhere when everything shows, and fold everywhere else, and where they haven't
    /// been read. What would show unless everything shows, and hasn't been read, is missing.
    pub(super) fn gap(&mut self, gap: &Unchanged) {
        let old = gap.old.clone();
        if old.is_empty() {
            return;
        }
        if self.shape.old.lines().is_none() {
            self.fold(gap, old);
            return;
        }
        let mut wanted = self.shape.opened.within(self.shape.commit, old.clone());
        let noted = self
            .shape
            .notes
            .iter()
            .filter_map(|note| if is_old(note.side) { Some(note.line) } else { gap.to_old(note.line) });
        wanted.extend(noted.filter(|line| old.contains(line)).map(|line| line..line + 1));
        let mut shown = if self.shape.everything { vec![old.clone()] } else { wanted.clone() };
        if let Old::InPart { known, .. } = self.shape.old {
            self.missing.extend(wanted.into_iter().flat_map(|lines| outside(lines, known)));
            shown = shown.into_iter().flat_map(|lines| inside(lines, known)).collect();
        }
        shown.sort_by_key(|lines| lines.start);
        let mut at = old.start;
        for lines in shown {
            if at < lines.start {
                self.fold(gap, at..lines.start);
            }
            for line in at.max(lines.start)..lines.end {
                self.same(line, gap.to_new(line));
            }
            at = at.max(lines.end);
        }
        if at < old.end {
            self.fold(gap, at..old.end);
        }
    }

    /// A fold of `old`, some of `gap`, and then the notes on its lines, which can't show.
    pub(super) fn fold(&mut self, gap: &Unchanged, old: Range<u32>) {
        let length = self.shape.old.lines();
        let (top, end, opens) =
            (old.start == 1, length.is_some_and(|length| old.end == length.saturating_add(1)), length.is_some());
        let new_start = gap.to_new(old.start);
        self.laid_out.push(Row::Fold(Fold { old: old.clone(), new_start, top, end, opens }));
        for line in old {
            self.notes_on(true, line);
            self.notes_on(false, gap.to_new(line));
        }
    }

    pub(super) fn hunk(&mut self, hunk: &Hunk) {
        let (mut old, mut new) = (hunk.old_start(), hunk.new_start());
        let (mut removed, mut added) = (Vec::new(), Vec::new());
        for line in hunk.lines() {
            match line.kind {
                LineKind::Same => {
                    self.changes(&mut removed, &mut added);
                    self.same(old, new);
                    (old, new) = (old + 1, new + 1);
                }
                LineKind::Removed => {
                    if !added.is_empty() {
                        self.changes(&mut removed, &mut added);
                    }
                    removed.push(old);
                    old += 1;
                }
                LineKind::Added => {
                    added.push(new);
                    new += 1;
                }
            }
        }
        self.changes(&mut removed, &mut added);
    }

    /// A run of lines removed and the lines added after them: in one column, each removed one and then each added
    /// one, and side by side, each removed one beside the added one that took its place.
    pub(super) fn changes(&mut self, removed: &mut Vec<u32>, added: &mut Vec<u32>) {
        let (removed, added) = (std::mem::take(removed), std::mem::take(added));
        if !self.shape.split {
            for old in removed {
                self.laid_out.push(Row::Line { kind: LineKind::Removed, old: Some(old), new: None });
                self.notes_on(true, old);
            }
            for new in added {
                self.laid_out.push(Row::Line { kind: LineKind::Added, old: None, new: Some(new) });
                self.notes_on(false, new);
            }
            return;
        }
        for ix in 0..removed.len().max(added.len()) {
            let (old, new) = (removed.get(ix).copied(), added.get(ix).copied());
            self.laid_out.push(Row::Pair { old, new, same: false });
            if let Some(old) = old {
                self.notes_on(true, old);
            }
            if let Some(new) = new {
                self.notes_on(false, new);
            }
        }
    }

    /// The rows, with the notes whose lines aren't there now at the end.
    pub(super) fn finish(mut self) -> Laid {
        let left = self.shape.notes.iter().zip(&self.placed).filter(|(_, placed)| !**placed);
        self.laid_out.extend(left.map(|(note, _)| note.row.clone()));
        self.missing.sort_by_key(|lines| lines.start);
        let missing = self.missing.into_iter().fold(Vec::<Range<u32>>::new(), |mut joined, lines| {
            match joined.last_mut() {
                Some(last) if last.end >= lines.start => last.end = last.end.max(lines.end),
                _ => joined.push(lines),
            }
            joined
        });
        Laid { rows: self.laid_out, missing }
    }
}

/// What of `lines` is in `known`, which is in order.
fn inside(lines: Range<u32>, known: &[Range<u32>]) -> impl Iterator<Item = Range<u32>> {
    known.iter().map(move |part| part.start.max(lines.start)..part.end.min(lines.end)).filter(|part| !part.is_empty())
}

/// What of `lines` isn't in `known`, which is in order.
fn outside(lines: Range<u32>, known: &[Range<u32>]) -> Vec<Range<u32>> {
    let mut at = lines.start;
    let mut missing = Vec::new();
    for part in known.iter().filter(|part| part.end > lines.start && part.start < lines.end) {
        if at < part.start {
            missing.push(at..part.start);
        }
        at = at.max(part.end);
    }
    if at < lines.end {
        missing.push(at..lines.end);
    }
    missing
}
