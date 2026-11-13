use crate::file_view::{HIGHLIGHT_UP_TO, Line, Lines, TAB};
use crate::patch::{FilePatch, Hunk, LineKind, PatchLine};
use gpui_kit::SharedString;
use protocol::model::PageLine;
use std::collections::HashMap;
use std::ops::Range;

#[derive(Clone, Debug, Default)]
pub(in super::super) struct Sides {
    pub(in super::super) old: Side,
    pub(in super::super) new: Side,
    pub(in super::super) extent: Extent,
    /// How many characters the longest line has, which sets how far the code scrolls sideways.
    pub(in super::super) widest: usize,
}

/// How much of the file as it was the sides have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in super::super) enum Extent {
    /// Only the hunks' lines, so nothing git left out can show.
    #[default]
    Hunks,
    /// Every one of its lines, which number this many.
    Whole(u32),
    /// The hunks' lines and those read of it a page at a time, of this many.
    InPart(u32),
}

impl Extent {
    /// How many lines it has, when that's known.
    pub(in super::super) fn lines(self) -> Option<u32> {
        match self {
            Self::Hunks => None,
            Self::Whole(lines) | Self::InPart(lines) => Some(lines),
        }
    }
}

/// Lines of the file as it was, read on their own rather than with the rest of it, from the number of the first.
#[derive(Clone, Debug)]
pub(in super::super) struct Run {
    pub(super) start: u32,
    pub(super) lines: Vec<Line>,
    /// Each line with a tab in it, as the file has it.
    pub(super) tabbed: Vec<(u32, SharedString)>,
}

impl Run {
    /// `text`'s lines, from line `start`, coloured as `language` if it's given, which is for a background thread.
    pub(in super::super) fn new(start: u32, text: &str, language: Option<&str>) -> Self {
        let numbered = (start..).zip(text.lines());
        let tabbed = numbered.filter(|(_, line)| line.contains('\t')).map(|(n, line)| (n, line.into())).collect();
        Self { start, lines: Lines::new(text, language).lines, tabbed }
    }

    /// A page of the file as it was, whose lines are `numbers`, with a line that was cut short ending in an ellipsis.
    pub(in super::super) fn from_page(numbers: &Range<u32>, page: &[PageLine], language: Option<&str>) -> Self {
        let text = page.iter().fold(String::new(), |mut text, line| {
            text.push_str(&line.text);
            text.push_str(if line.cut.is_some() { "…\n" } else { "\n" });
            text
        });
        Self::new(numbers.start, &text, language)
    }

    /// The lines it has, by number.
    pub(in super::super) fn numbers(&self) -> Range<u32> {
        self.start..self.start.saturating_add(count(&self.lines))
    }
}

/// The lines of a side that are known, in runs from their first line's number, in order and apart from each other.
#[derive(Clone, Debug, Default)]
pub(in super::super) struct Side {
    pub(super) runs: Vec<(u32, Vec<Line>)>,
    /// The words that changed within a line a change removed or added, as `look::word_changes` finds them, by byte
    /// range of its text as it's drawn. A line with none isn't here.
    pub(super) changed: HashMap<u32, Vec<Range<usize>>>,
    /// Each line with a tab in it, as the file has it, since it's drawn with spaces for its tabs.
    pub(super) tabbed: HashMap<u32, SharedString>,
}

impl Side {
    /// Each run of `texts` from the number of its first line, coloured as `language` if it's given.
    pub(super) fn new(texts: impl IntoIterator<Item = (u32, String)>, language: Option<&str>) -> Self {
        let mut side = Self::default();
        for (start, text) in texts {
            let run = Run::new(start, &text, language);
            side.insert(run.start, &run.lines, run.tabbed);
        }
        side
    }

    /// Lines numbered from `start`, apart from those it has already.
    pub(super) fn insert(&mut self, start: u32, lines: &[Line], tabbed: impl IntoIterator<Item = (u32, SharedString)>) {
        let end = start.saturating_add(count(lines));
        let (mut at, mut next) = (start, self.runs.partition_point(|(first, run)| first + count(run) <= start));
        let mut missing = Vec::new();
        while at < end {
            let to = match self.runs.get(next).map(|(first, run)| *first..first + count(run)) {
                Some(run) if run.start <= at => {
                    (at, next) = (run.end, next + 1);
                    continue;
                }
                Some(run) => run.start.min(end),
                None => end,
            };
            missing.push(at..to);
            at = to;
        }
        for lines_missing in &missing {
            let ix = |number: u32| usize::try_from(number - start).unwrap_or(usize::MAX);
            let run = lines[ix(lines_missing.start)..ix(lines_missing.end)].to_vec();
            let place = self.runs.partition_point(|(first, _)| *first < lines_missing.start);
            self.runs.insert(place, (lines_missing.start, run));
        }
        let new = tabbed.into_iter().filter(|(number, _)| missing.iter().any(|lines| lines.contains(number)));
        self.tabbed.extend(new);
    }

    /// The lines it has, in runs, in order.
    pub(super) fn known(&self) -> impl Iterator<Item = Range<u32>> {
        self.runs.iter().map(|(first, run)| *first..first + count(run))
    }

    /// The part of line `number` at `range` of its text as it's drawn, as the file has it.
    pub(in super::super) fn as_in_the_file(&self, number: u32, range: Range<usize>) -> Option<String> {
        let Some(original) = self.tabbed.get(&number) else {
            return self.line(number)?.text.get(range).map(ToString::to_string);
        };
        // How far into the line as the file has it a place in the line as it's drawn is.
        let in_the_file = |drawn: usize| {
            let mut so_far = 0;
            let found = original.char_indices().find(|(_, character)| {
                let past = so_far >= drawn;
                so_far += if *character == '\t' { TAB.len() } else { character.len_utf8() };
                past
            });
            found.map_or(original.len(), |(ix, _)| ix)
        };
        original.get(in_the_file(range.start)..in_the_file(range.end)).map(ToString::to_string)
    }

    pub(in super::super) fn line(&self, number: u32) -> Option<&Line> {
        let run = self.runs.partition_point(|(start, _)| *start <= number).checked_sub(1)?;
        let (start, lines) = &self.runs[run];
        lines.get(usize::try_from(number - start).ok()?)
    }

    /// The text of line `number` as it's drawn, or nothing when it doesn't have it.
    pub(super) fn text(&self, number: u32) -> &str {
        self.line(number).map_or("", |line| line.text.as_ref())
    }

    pub(in super::super) fn changed(&self, number: u32) -> &[Range<usize>] {
        self.changed.get(&number).map_or(&[], Vec::as_slice)
    }

    pub(super) fn widest(&self) -> usize {
        self.runs.iter().flat_map(|(_, lines)| lines).map(|line| line.text.chars().count()).max().unwrap_or(0)
    }
}

impl Sides {
    /// From the hunks alone, as they show before anything's been made: without colours, and with nothing marked within
    /// their lines, since both are made off the main thread.
    pub(in super::super) fn plain(file: &FilePatch) -> Self {
        Self::of_hunks(file, None)
    }

    /// From the hunks alone, coloured as `language` if it's given, which is for a background thread. A file the change
    /// adds or removes is whole this way, since its hunk is all of it.
    pub(in super::super) fn from_patch(file: &FilePatch, language: Option<&str>) -> Self {
        Self::of_hunks(file, language).marked(file)
    }

    pub(super) fn of_hunks(file: &FilePatch, language: Option<&str>) -> Self {
        let side = |kept: LineKind, start: fn(&Hunk) -> u32| {
            let runs = file.hunks().iter().filter_map(|hunk| {
                let text = joined(hunk.lines().iter().filter(|line| line.kind == kept || line.kind == LineKind::Same));
                (!text.is_empty()).then(|| (start(hunk), text))
            });
            Side::new(runs, language)
        };
        let (old, new) = (side(LineKind::Removed, Hunk::old_start), side(LineKind::Added, Hunk::new_start));
        let extent = match (file.old_path(), file.new_path()) {
            (None, _) => Extent::Whole(0),
            (_, None) => Extent::Whole(file.hunks().last().map_or(0, |hunk| hunk.old_end() - 1)),
            _ => Extent::Hunks,
        };
        Self::sized(old, new, extent)
    }

    /// From `base`, the file as it was, and the hunks, which make the file as it is. When the hunks don't match it, as
    /// when it was read from the wrong commit, it's left out, since the hunks can be trusted.
    pub(in super::super) fn with_base(file: &FilePatch, base: &str, language: Option<&str>) -> Self {
        let Some(new) = changed_into(base, file.hunks()) else { return Self::from_patch(file, language) };
        let language = language.filter(|_| base.len().max(new.len()) <= HIGHLIGHT_UP_TO);
        let length = u32::try_from(base.lines().count()).unwrap_or(u32::MAX);
        let (old, new) = (Side::new([(1, base.to_string())], language), Side::new([(1, new)], language));
        Self::sized(old, new, Extent::Whole(length)).marked(file)
    }

    /// From the hunks, coloured as `language` if it's given, for a file as it was that has `lines` lines and is too big
    /// to be read whole, so its other lines are added as they're read.
    pub(in super::super) fn in_part(file: &FilePatch, lines: u32, language: Option<&str>) -> Self {
        Self { extent: Extent::InPart(lines), ..Self::from_patch(file, language) }
    }

    /// Adds lines of the file as it was that were read on their own, to both sides where they're between the hunks,
    /// since what's there is the same on both.
    pub(in super::super) fn add(&mut self, file: &FilePatch, run: &Run) {
        let numbers = run.numbers();
        for gap in file.unchanged(u32::MAX) {
            let lines = gap.old.start.max(numbers.start)..gap.old.end.min(numbers.end);
            if lines.is_empty() {
                continue;
            }
            let ix = |number: u32| usize::try_from(number - run.start).unwrap_or(usize::MAX);
            let part = &run.lines[ix(lines.start)..ix(lines.end)];
            let tabbed = run.tabbed.iter().filter(|(number, _)| lines.contains(number)).cloned();
            self.old.insert(lines.start, part, tabbed.clone());
            self.new.insert(gap.to_new(lines.start), part, tabbed.map(|(number, text)| (gap.to_new(number), text)));
        }
        let widest = run.lines.iter().map(|line| line.text.chars().count()).max().unwrap_or(0);
        self.widest = self.widest.max(widest);
    }

    /// The lines of the file as it was that it has, by number, in order, when it has them in part.
    pub(in super::super) fn known(&self) -> Option<Vec<Range<u32>>> {
        matches!(self.extent, Extent::InPart(_)).then(|| self.old.known().collect())
    }

    pub(super) fn sized(old: Side, new: Side, extent: Extent) -> Self {
        let widest = old.widest().max(new.widest());
        Self { old, new, extent, widest }
    }

    /// With the words that changed within the hunks' lines marked, as `look::word_changes` finds them in their text as
    /// it's drawn.
    pub(super) fn marked(mut self, file: &FilePatch) -> Self {
        for hunk in file.hunks() {
            let lines = numbered(hunk);
            let marks = {
                let changes: Vec<(look::LineChange, &str)> = (lines.iter())
                    .map(|(kind, number)| match kind {
                        LineKind::Same => (look::LineChange::Same, ""),
                        LineKind::Removed => (look::LineChange::Removed, self.old.text(*number)),
                        LineKind::Added => (look::LineChange::Added, self.new.text(*number)),
                    })
                    .collect();
                look::word_changes(&changes)
            };
            for ((kind, number), marks) in lines.into_iter().zip(marks).filter(|(_, marks)| !marks.is_empty()) {
                let side = if kind == LineKind::Removed { &mut self.old } else { &mut self.new };
                side.changed.insert(number, marks);
            }
        }
        self
    }
}

fn count(lines: &[Line]) -> u32 {
    u32::try_from(lines.len()).unwrap_or(u32::MAX)
}

/// Each line with a line ending after it, so a last line that's empty is still a line.
fn joined<'a>(lines: impl Iterator<Item = &'a PatchLine>) -> String {
    lines.fold(String::new(), |mut text, line| {
        text.push_str(&line.text);
        text.push('\n');
        text
    })
}

/// The file as it is, from `base` with `hunks` applied, or `None` when a line the hunks say `base` has isn't there.
fn changed_into(base: &str, hunks: &[Hunk]) -> Option<String> {
    let old: Vec<&str> = base.lines().collect();
    let line = |number: u32| old.get(usize::try_from(number).ok()?.checked_sub(1)?).copied();
    let (mut new, mut next) = (String::with_capacity(base.len()), 1);
    for hunk in hunks {
        for number in next..hunk.old_start() {
            new.push_str(line(number)?);
            new.push('\n');
        }
        let mut at = hunk.old_start();
        for patch_line in hunk.lines() {
            if patch_line.kind != LineKind::Added {
                (line(at)? == patch_line.text.as_ref()).then_some(())?;
                at += 1;
            }
            if patch_line.kind != LineKind::Removed {
                new.push_str(&patch_line.text);
                new.push('\n');
            }
        }
        next = hunk.old_end();
    }
    let rest = old.get(usize::try_from(next).ok()?.checked_sub(1)?..)?;
    for text in rest {
        new.push_str(text);
        new.push('\n');
    }
    Some(new)
}

/// Each of a hunk's lines, by its number on its side: the old side for a line it removes, and the new otherwise.
fn numbered(hunk: &Hunk) -> Vec<(LineKind, u32)> {
    let (mut old, mut new) = (hunk.old_start(), hunk.new_start());
    let numbered = hunk.lines().iter().map(|line| {
        let number = if line.kind == LineKind::Removed { old } else { new };
        old += u32::from(line.kind != LineKind::Added);
        new += u32::from(line.kind != LineKind::Removed);
        (line.kind, number)
    });
    numbered.collect()
}
