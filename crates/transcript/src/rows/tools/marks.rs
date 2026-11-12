//! The words that changed in a diff's lines, worked out once for each diff and kept, since a call's lines are drawn on
//! every frame while it's open, and comparing them takes far longer than drawing them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::rc::Rc;

use crate::calls::diff;
use crate::calls::lines::{Mark, NumberedLine, Section};

/// How many diffs' marks are kept.
const KEPT: usize = 128;

thread_local! {
    /// By a hash of the lines, which are compared too, since two diffs can share a hash.
    static KEPT_MARKS: RefCell<HashMap<u64, Rc<Vec<Section>>>> = RefCell::new(HashMap::new());
}

/// `sections` with the words that changed in them marked. Sections with nothing to compare, as a new file has, are
/// given back as they are.
pub(super) fn marked(sections: &Rc<Vec<Section>>) -> Rc<Vec<Section>> {
    let replaced = |section: &Section| {
        let has = |mark| section.iter().any(|line| line.mark == mark);
        has(Mark::Removed) && has(Mark::Added)
    };
    if !sections.iter().any(replaced) {
        return sections.clone();
    }
    let mut hasher = DefaultHasher::new();
    sections.hash(&mut hasher);
    let key = hasher.finish();
    KEPT_MARKS.with_borrow_mut(|kept| {
        if let Some(found) = kept.get(&key).filter(|found| same_lines(found, sections)) {
            return found.clone();
        }
        let marked: Rc<Vec<Section>> = Rc::new(sections.iter().cloned().map(diff::mark_changes).collect());
        if kept.len() >= KEPT {
            kept.clear();
        }
        kept.insert(key, marked.clone());
        marked
    })
}

/// Whether `marked` is `sections` with its words marked.
fn same_lines(marked: &[Section], sections: &[Section]) -> bool {
    marked.len() == sections.len()
        && marked.iter().zip(sections).all(|(a, b)| a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b)))
}

fn same(a: &NumberedLine, b: &NumberedLine) -> bool {
    (a.mark, a.number, &a.text) == (b.mark, b.number, &b.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::diff::from_text;

    /// What's marked of the added line of a one-line change.
    fn marked_on_added(sections: &[Section]) -> Vec<&str> {
        let line = &sections[0][1];
        line.changed.iter().map(|range| &line.text[range.clone()]).collect()
    }

    #[test]
    fn a_diff_s_marks_are_worked_out_once_and_kept() {
        let sections = Rc::new(vec![from_text("let a = 1;", "let a = 2;")]);

        let (first, again) = (marked(&sections), marked(&Rc::new(sections.to_vec())));

        assert!(Rc::ptr_eq(&first, &again), "the same lines get what was kept");
        assert_eq!(marked_on_added(&first), ["2"]);
        let other = marked(&Rc::new(vec![from_text("let a = 1;", "let b = 1;")]));
        assert_eq!(marked_on_added(&other), ["b"], "other lines get their own");
    }

    #[test]
    fn sections_with_nothing_to_compare_are_given_back_as_they_are() {
        let sections = Rc::new(vec![from_text("", "new\nlines")]);

        assert!(Rc::ptr_eq(&marked(&sections), &sections));
    }
}
