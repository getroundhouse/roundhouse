use super::split;
use similar::{Algorithm, DiffOp, DiffTag, capture_diff_slices};
use split::Split;
use std::ops::Range;

/// What's marked on each of a run's lines, as byte ranges of its text, in order.
type Marked = Vec<Vec<Range<usize>>>;

/// How a line of a diff changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineChange {
    Same,
    Removed,
    Added,
}

/// The words that changed within each of `lines`, a diff's lines in order, as byte ranges of its text, in order. Each
/// run of removed lines is compared with the run of added lines straight after it, which took its place, so a line
/// that's the same on both sides, or in a run with none to compare with, has none.
pub fn word_changes(lines: &[(LineChange, &str)]) -> Vec<Vec<Range<usize>>> {
    let mut marks = vec![Vec::new(); lines.len()];
    let mut at = 0;
    while at < lines.len() {
        let run = |from: usize, change| lines[from..].iter().take_while(|(line, _)| *line == change).count();
        let removed = run(at, LineChange::Removed);
        let added = run(at + removed, LineChange::Added);
        let (old, new) = lines[at..at + removed + added].split_at(removed);
        let (lost, gained) = compared(&texts(old), &texts(new));
        for (marked, changed) in marks[at..].iter_mut().zip(lost.into_iter().chain(gained)) {
            *marked = changed;
        }
        at += (removed + added).max(1);
    }
    marks
}

fn texts<'a>(lines: &[(LineChange, &'a str)]) -> Vec<&'a str> {
    lines.iter().map(|(_, text)| *text).collect()
}

/// What changed within `removed`, a run of removed lines, and `added`, the run of added lines that took its place,
/// line by line. Nothing is marked when either is empty or too long to compare, or when they have nothing in common
/// but space, since then every word changed.
pub(super) fn compared(removed: &[&str], added: &[&str]) -> (Marked, Marked) {
    let mut marks = (vec![Vec::new(); removed.len()], vec![Vec::new(); added.len()]);
    if removed.is_empty() || added.is_empty() {
        return marks;
    }
    let start = shared_start(&[removed, added].concat());
    let (Some(old), Some(new)) = (Split::of(removed, start), Split::of(added, start)) else { return marks };
    if old.is_empty() || new.is_empty() {
        return marks;
    }
    let stretches = cleaned(stretches(&capture_diff_slices(Algorithm::Patience, old.words(), new.words())), &old, &new);
    if !stretches.iter().any(|stretch| stretch.diffed == Diffed::Kept && old.weight(&stretch.old) > 0) {
        return marks;
    }
    for stretch in stretches.iter().filter(|stretch| stretch.diffed == Diffed::Changed) {
        old.mark(&stretch.old, &mut marks.0);
        new.mark(&stretch.new, &mut marks.1);
    }
    let (old_realigned, new_realigned) = realigned(&stretches, &old, &new);
    let tidy = |lines: &mut [Vec<Range<usize>>], texts: &[&str], realigned: &[bool]| {
        for ((ranges, text), realigned) in lines.iter_mut().zip(texts).zip(realigned) {
            *ranges = tidied(text, std::mem::take(ranges), *realigned);
        }
    };
    tidy(&mut marks.0, removed, &old_realigned);
    tidy(&mut marks.1, added, &new_realigned);
    marks
}

/// Whether the diff kept or changed a stretch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Diffed {
    Kept,
    Changed,
}

/// A stretch of a run's words that the diff kept on both sides, or changed, by which words it covers on each side.
struct Stretch {
    pub(super) diffed: Diffed,
    pub(super) old: Range<usize>,
    pub(super) new: Range<usize>,
}

/// The diff's stretches, with what it removed and added between two kept stretches as one changed stretch. Where each
/// starts is counted from the lengths, since similar can give the wrong place on the empty side of what it removes or
/// adds: `["b", "a"]` into `["a", "a"]` adds at old word 1 rather than 2.
fn stretches(ops: &[DiffOp]) -> Vec<Stretch> {
    let mut stretches: Vec<Stretch> = Vec::new();
    let (mut old_at, mut new_at) = (0, 0);
    for op in ops {
        let (tag, old, new) = op.as_tag_tuple();
        let (old, new) = (old_at..old_at + old.len(), new_at..new_at + new.len());
        (old_at, new_at) = (old.end, new.end);
        let diffed = if tag == DiffTag::Equal { Diffed::Kept } else { Diffed::Changed };
        match stretches.last_mut() {
            Some(last) if last.diffed == Diffed::Changed && diffed == Diffed::Changed => {
                (last.old.end, last.new.end) = (old.end, new.end);
            }
            _ => stretches.push(Stretch { diffed, old, new }),
        }
    }
    stretches
}

/// `stretches`, with each kept one that weighs no more than what changed on either side of it changed too, as
/// diff-match-patch's semantic cleanup does. Otherwise a word two texts share by chance, such as `the` between two
/// rewritten clauses, breaks what changed into pieces. What's still kept is what the change didn't touch. Weight is
/// in characters other than space, so a tab weighs what the spaces it's drawn as do.
fn cleaned(mut stretches: Vec<Stretch>, old: &Split, new: &Split) -> Vec<Stretch> {
    let changed = |stretch: &Stretch| old.weight(&stretch.old).max(new.weight(&stretch.new));
    let mut i = 1;
    while i + 1 < stretches.len() {
        let (before, kept, after) = (&stretches[i - 1], &stretches[i], &stretches[i + 1]);
        let weight = old.weight(&kept.old);
        let between = before.diffed == Diffed::Changed && after.diffed == Diffed::Changed;
        if kept.diffed == Diffed::Kept && between && weight <= changed(before) && weight <= changed(after) {
            let (old, new) = (before.old.start..after.old.end, before.new.start..after.new.end);
            stretches.splice(i - 1..=i + 1, [Stretch { diffed: Diffed::Changed, old, new }]);
            // The changed stretch is bigger now, so the kept one before it may weigh no more than it.
            i = i.saturating_sub(2).max(1);
        } else {
            i += 1;
        }
    }
    stretches
}

/// Which lines on each side start with a word kept at the start of a line on the other side, which are the only lines
/// whose indentation is marked when it changed. A line whose first word went to the middle of one, as when lines are
/// joined, only lost a line break.
fn realigned(stretches: &[Stretch], old: &Split, new: &Split) -> (Vec<bool>, Vec<bool>) {
    let (mut old_lines, mut new_lines) = (vec![false; old.lines()], vec![false; new.lines()]);
    for stretch in stretches.iter().filter(|stretch| stretch.diffed == Diffed::Kept) {
        for (o, n) in stretch.old.clone().zip(stretch.new.clone()) {
            if let (Some(o), Some(n)) = (old.line_it_starts(o), new.line_it_starts(n)) {
                (old_lines[o], new_lines[n]) = (true, true);
            }
        }
    }
    (old_lines, new_lines)
}

/// How many bytes every line starts with that are its indentation and comment marks, such as `    // `, which are
/// left out of what's compared, since they'd match across lines, and a run with fewer lines would mark one line's.
/// Only what isn't part of a word counts, so a word is never cut. A line that's no more than the start, such as an
/// empty comment line or an empty line, doesn't shorten it.
pub(super) fn shared_start(lines: &[&str]) -> usize {
    let Some(first) = lines.iter().find(|line| line.contains(split::in_a_word)) else { return 0 };
    let marks = first.len() - first.trim_start_matches(|c: char| !split::in_a_word(c)).len();
    lines.iter().fold(marks, |shared, line| {
        let differs = first[..shared].char_indices().zip(line.chars()).find(|((_, a), b)| a != b);
        differs.map_or(shared, |((at, _), _)| at)
    })
}

/// `ranges` of `text`, made easier to read: marks with only space between them are joined, and each is trimmed of
/// space at its ends. A mark of nothing but space is kept only as indentation that changed, on a line `realigned`
/// says starts on both sides, or as space at the end of the line. Between words, it's where one run broke a line and
/// the other didn't.
fn tidied(text: &str, ranges: Vec<Range<usize>>, realigned: bool) -> Vec<Range<usize>> {
    let mut joined: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match joined.last_mut() {
            Some(last) if text[last.end..range.start].trim().is_empty() => last.end = range.end,
            _ => joined.push(range),
        }
    }
    let trimmed = joined.into_iter().filter_map(|range| {
        let marked = &text[range.clone()];
        let words = marked.trim();
        if words.is_empty() {
            let indentation = realigned && text[..range.start].trim().is_empty();
            return (indentation || text[range.end..].trim().is_empty()).then_some(range);
        }
        let start = range.start + marked.len() - marked.trim_start().len();
        Some(start..start + words.len())
    });
    trimmed.collect()
}
