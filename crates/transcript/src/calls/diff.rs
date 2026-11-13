//! A change to a file, as lines to show: from the patch the agent sent back, with the file's own line numbers, or
//! worked out from the call's text when there's no patch yet, and the words that changed within the lines it removed
//! and added.

use protocol::event::{Hunk, LineMark};
use similar::{ChangeTag, TextDiff};

use super::lines::{Counts, Mark, NumberedLine, Section};

/// Past this many lines on a side, a change worked out from text shows as everything removed, then everything added.
/// It's worked out where it's drawn, and comparing two long texts that share little takes as long as their lengths
/// multiplied.
const COMPARED_LINES: usize = 1000;

/// The sections of the patch the agent sent back, one for each hunk.
pub(crate) fn from_hunks(hunks: &[Hunk]) -> Vec<Section> {
    hunks
        .iter()
        .map(|hunk| {
            let (mut old, mut new) = (hunk.old_start, hunk.new_start);
            let lines = hunk.lines.iter().filter_map(|line| {
                let mark = mark(line.mark)?;
                let number = if mark == Mark::Removed { &mut old } else { &mut new };
                let shown = NumberedLine::new(mark, Some(*number), &line.text);
                *number += 1;
                if mark == Mark::Same {
                    old += 1;
                }
                Some(shown)
            });
            lines.collect()
        })
        .collect()
}

/// The lines of replacing `old` with `new`, worked out from the text alone. Removed lines come before what replaced
/// them, so the words that changed in a run of removed lines can be compared with the run that replaced it.
pub(crate) fn from_text(old: &str, new: &str) -> Section {
    let (old, new): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    if old.len() > COMPARED_LINES || new.len() > COMPARED_LINES {
        let removed = old.iter().map(|text| NumberedLine::new(Mark::Removed, None, text));
        return removed.chain(new.iter().map(|text| NumberedLine::new(Mark::Added, None, text))).collect();
    }
    let diff = TextDiff::from_slices(&old, &new);
    let lines = diff.iter_all_changes().map(|change| {
        let mark = match change.tag() {
            ChangeTag::Equal => Mark::Same,
            ChangeTag::Delete => Mark::Removed,
            ChangeTag::Insert => Mark::Added,
        };
        NumberedLine::new(mark, None, change.value())
    });
    lines.collect()
}

/// How many lines a patch added and removed, without copying it.
pub(crate) fn change_counts(hunks: &[Hunk]) -> Counts {
    Counts::of(hunks.iter().flat_map(|hunk| &hunk.lines).filter_map(|line| mark(line.mark)))
}

/// How many lines these sections added and removed.
pub(crate) fn counts(sections: &[Section]) -> Counts {
    Counts::of(sections.iter().flatten().map(|line| line.mark))
}

/// A patch's mark for a line. One this build doesn't know is left out, since there's no telling where it goes.
fn mark(mark: LineMark) -> Option<Mark> {
    match mark {
        LineMark::Same => Some(Mark::Same),
        LineMark::Removed => Some(Mark::Removed),
        LineMark::Added => Some(Mark::Added),
        LineMark::Unknown => None,
    }
}

/// `section`, with the words that changed marked in each run of removed lines and the run of added lines after it,
/// which took its place.
pub(crate) fn mark_changes(mut section: Section) -> Section {
    let lines: Vec<(look::LineChange, &str)> = (section.iter())
        .map(|line| {
            let change = match line.mark {
                Mark::Same => look::LineChange::Same,
                Mark::Removed => look::LineChange::Removed,
                Mark::Added => look::LineChange::Added,
            };
            (change, line.text.as_str())
        })
        .collect();
    let marks = look::word_changes(&lines);
    for (line, changed) in section.iter_mut().zip(marks) {
        line.changed = changed;
    }
    section
}

#[cfg(test)]
mod tests {
    use protocol::event::{Hunk, HunkLine, LineMark};

    use super::{Counts, Mark, NumberedLine, counts, from_hunks, from_text, mark_changes};

    fn line(mark: LineMark, text: &str) -> HunkLine {
        HunkLine { mark, text: text.to_string() }
    }

    #[test]
    fn a_patch_s_lines_are_numbered_as_in_the_file() {
        let hunks = [Hunk {
            old_start: 29,
            new_start: 30,
            lines: vec![
                line(LineMark::Same, "Self::Clear => \"clear\","),
                line(LineMark::Removed, "Self::Caution => \"caution\","),
                line(LineMark::Added, "Self::Caution => \"approach with caution\","),
                line(LineMark::Unknown, "a line this build can't place"),
                line(LineMark::Same, "Self::Stop => \"stop\","),
            ],
        }];

        let sections = from_hunks(&hunks);
        let marked = mark_changes(sections[0].clone());

        let numbers: Vec<_> = sections[0].iter().map(|line| (line.mark, line.number)).collect();
        assert_eq!(
            numbers,
            [(Mark::Same, Some(30)), (Mark::Removed, Some(30)), (Mark::Added, Some(31)), (Mark::Same, Some(32))]
        );
        let added = &marked[2];
        assert_eq!(added.changed.iter().map(|range| &added.text[range.clone()]).collect::<Vec<_>>(), ["approach with"]);
        assert!(marked[1].changed.is_empty(), "nothing was taken out of the old line");
        assert_eq!(counts(&sections), Counts { added: 1, removed: 1 });
    }

    #[test]
    fn a_change_worked_out_from_text_keeps_what_both_sides_share() {
        let section = from_text("a\nb\nc\nd", "a\nB\nc\nnew\nd");

        let marks: Vec<_> = section.iter().map(|line| (line.mark, line.text.as_str())).collect();
        assert_eq!(
            marks,
            [
                (Mark::Same, "a"),
                (Mark::Removed, "b"),
                (Mark::Added, "B"),
                (Mark::Same, "c"),
                (Mark::Added, "new"),
                (Mark::Same, "d"),
            ]
        );
        assert!(section.iter().all(|line| line.number.is_none()), "the file's numbers aren't known");
        assert!(mark_changes(section)[1].changed.is_empty(), "lines with nothing in common changed whole");
    }

    #[test]
    fn a_long_change_still_shows_only_what_changed() {
        let old = Vec::from_fn(900, |i| format!("line {i}"));
        let mut new = old.clone();
        new[450] = "a new line".to_string();

        let section = from_text(&old.join("\n"), &new.join("\n"));

        assert_eq!(counts(&[section]), Counts { added: 1, removed: 1 });
    }

    #[test]
    fn a_huge_change_shows_as_removed_then_added() {
        let old = "x\n".repeat(1500);

        let section = from_text(&old, "y");

        assert_eq!(section.iter().filter(|line| line.mark == Mark::Removed).count(), 1500);
        assert_eq!(section.last().unwrap().mark, Mark::Added);
    }

    #[test]
    fn the_words_that_changed_are_marked_in_runs_of_any_length() {
        let section = mark_changes(from_text("# one two\n# three", "# one three"));

        let marked =
            |line: &NumberedLine| line.changed.iter().map(|range| line.text[range.clone()].to_string()).collect();
        let marks: Vec<Vec<String>> = section.iter().map(marked).collect();
        assert_eq!(marks, [vec!["two".to_string()], vec![], vec![]], "`three` only moved");
    }
}
