//! A change to a file, as lines to show: from the patch the agent sent back, with the file's own line numbers, or
//! worked out from the call's text when there's no patch yet. Within a line that changed, the part that changed is
//! picked out.

use std::ops::Range;

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
            pick_out_changes(lines.collect())
        })
        .collect()
}

/// The lines of replacing `old` with `new`, worked out from the text alone. Removed lines come before what replaced
/// them, so a line changed in place pairs with its new version.
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
    pick_out_changes(lines.collect())
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

/// Pairs each run of removed lines with the run of added lines after it, line by line, and marks what changed in
/// each pair: everything between the start and end they share.
fn pick_out_changes(mut lines: Section) -> Section {
    let mut i = 0;
    while i < lines.len() {
        let removed = lines[i..].iter().take_while(|l| l.mark == Mark::Removed).count();
        let added = lines[i + removed..].iter().take_while(|l| l.mark == Mark::Added).count();
        if removed == added {
            for k in 0..removed {
                let (old, new) = changed_ranges(&lines[i + k].text, &lines[i + removed + k].text);
                lines[i + k].changed = old;
                lines[i + removed + k].changed = new;
            }
        }
        i += (removed + added).max(1);
    }
    lines
}

/// The changed middle of each of two lines, or nothing when they share no start or end, since then the whole line
/// changed.
fn changed_ranges(old: &str, new: &str) -> (Option<Range<usize>>, Option<Range<usize>>) {
    let start: usize = old.chars().zip(new.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
    let (old_rest, new_rest) = (&old[start..], &new[start..]);
    let end: usize =
        old_rest.chars().rev().zip(new_rest.chars().rev()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
    if start == 0 && end == 0 {
        return (None, None);
    }
    let range = |text: &str| (start < text.len() - end).then(|| start..text.len() - end);
    (range(old), range(new))
}

#[cfg(test)]
mod tests {
    use protocol::event::{Hunk, HunkLine, LineMark};

    use super::{Counts, Mark, counts, from_hunks, from_text};

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

        let numbers: Vec<_> = sections[0].iter().map(|line| (line.mark, line.number)).collect();
        assert_eq!(
            numbers,
            [(Mark::Same, Some(30)), (Mark::Removed, Some(30)), (Mark::Added, Some(31)), (Mark::Same, Some(32))]
        );
        let added = &sections[0][2];
        assert_eq!(&added.text[added.changed.clone().unwrap()], "approach with ");
        assert_eq!(sections[0][1].changed, None, "nothing was taken out of the old line");
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
        assert_eq!(section[1].changed, None, "lines with nothing in common changed whole");
    }

    #[test]
    fn a_long_change_still_shows_only_what_changed() {
        let old: Vec<String> = (0..900).map(|i| format!("line {i}")).collect();
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
    fn a_changed_letter_in_a_wide_character_line_is_found_on_a_character_boundary() {
        let section = from_text("héllo wörld", "héllo wérld");

        assert_eq!(&section[1].text[section[1].changed.clone().unwrap()], "é");
    }
}
