//! What changed within a line that another took the place of, word by word, so a small edit to a long line stands out.
//! Lines with nothing but space in common show as changed throughout, as there's nothing to pick out.

use std::ops::Range;

use similar::{ChangeTag, TextDiff};

/// Longer lines than this, in bytes, show as changed throughout, since comparing them word by word could take a
/// noticeable moment, and a line that long is rarely read word by word.
const LONGEST: usize = 1000;

/// Where in a line it changed, by byte range, in order.
pub(super) type Changed = Vec<Range<usize>>;

/// What `old` lost and `new` gained, or `None` when nothing beyond space stayed.
pub(super) fn changed(old: &str, new: &str) -> Option<(Changed, Changed)> {
    if old.len() > LONGEST || new.len() > LONGEST {
        return None;
    }
    let diff = TextDiff::from_words(old, new);
    let (mut lost, mut gained) = (Vec::new(), Vec::new());
    let (mut in_old, mut in_new, mut kept) = (0, 0, false);
    for change in diff.iter_all_changes() {
        let length = change.value().len();
        match change.tag() {
            ChangeTag::Equal => {
                kept |= !change.value().trim().is_empty();
                (in_old, in_new) = (in_old + length, in_new + length);
            }
            ChangeTag::Delete => {
                join(&mut lost, in_old..in_old + length);
                in_old += length;
            }
            ChangeTag::Insert => {
                join(&mut gained, in_new..in_new + length);
                in_new += length;
            }
        }
    }
    kept.then_some((lost, gained))
}

/// Adds `range`, as part of the last one when it carries straight on from it.
fn join(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    match ranges.last_mut() {
        Some(last) if last.end == range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_words_that_changed_are_picked_out() {
        let (old, new) = ("let total = count + 1;", "let total = count * 2;");

        let (lost, gained) = changed(old, new).unwrap();

        let picked = |text: &str, ranges: &[Range<usize>]| -> String {
            ranges.iter().map(|range| &text[range.clone()]).collect::<String>().replace(' ', "")
        };
        assert_eq!((picked(old, &lost), picked(new, &gained)), ("+1;".to_string(), "*2;".to_string()));
    }

    #[test]
    fn lines_with_only_space_in_common_change_throughout() {
        assert_eq!(changed("one two", "three four"), None);
        assert_eq!(changed(&"same a".repeat(200), &"same b".repeat(200)), None, "too long to compare");
    }
}
