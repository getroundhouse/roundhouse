use super::*;
use crate::patch::parse;

fn file(patch: &str) -> FilePatch {
    parse(patch).unwrap().remove(0)
}

#[test]
fn the_file_as_it_was_and_the_hunks_make_both_sides_whole() {
    let changed = file("--- a/f\n+++ b/f\n@@ -2,3 +2,3 @@\n b\n-c\n+C\n d\n");

    let sides = Sides::with_base(&changed, "a\nb\nc\nd\ne\n", None);

    let text = |side: &Side, number| side.line(number).map(|line| line.text.to_string()).unwrap_or_default();
    assert_eq!(sides.extent, Extent::Whole(5));
    assert_eq!([text(&sides.old, 1), text(&sides.new, 3), text(&sides.new, 5)], ["a", "C", "e"]);
}

#[test]
fn a_file_as_it_was_that_doesnt_match_the_hunks_is_left_out() {
    let changed = file("--- a/f\n+++ b/f\n@@ -2,3 +2,3 @@\n b\n-c\n+C\n d\n");

    let sides = Sides::with_base(&changed, "a\nB\nc\nd\n", None);

    assert_eq!((sides.extent, sides.old.line(1).is_none()), (Extent::Hunks, true), "only the hunks' lines");
}

#[test]
fn a_line_with_tabs_is_drawn_with_spaces_and_copied_with_its_tabs() {
    let changed = file("--- a/f\n+++ b/f\n@@ -1 +1 @@\n-\tlet x = 1;\n+\tlet x = 2;\n");

    let sides = Sides::from_patch(&changed, None);

    assert_eq!(sides.new.line(1).map(|line| line.text.as_ref()), Some("    let x = 2;"));
    assert_eq!(sides.new.as_in_the_file(1, 4..7).as_deref(), Some("let"));
    assert_eq!(sides.new.as_in_the_file(1, 0..14).as_deref(), Some("\tlet x = 2;"));
}

#[test]
fn lines_read_on_their_own_go_on_both_sides_between_the_hunks() {
    // Line 3 becomes two lines, so what's after it is a line further down on the new side.
    let changed = file("--- a/f\n+++ b/f\n@@ -2,3 +2,4 @@\n b\n-c\n+C\n+C2\n d\n");
    let mut sides = Sides::in_part(&changed, 10, None);
    let text = |side: &Side, number| side.line(number).map(|line| line.text.to_string());

    sides.add(&changed, &Run::new(4, "D?\ne\n\tf\n", None));
    sides.add(&changed, &Run::new(6, "\tf\ng\n", None));

    assert_eq!(sides.known(), Some(vec![2..5, 5..7, 7..8]), "the hunk's lines, and those read after it");
    assert_eq!([text(&sides.old, 5), text(&sides.new, 6)], [Some("e".into()), Some("e".into())]);
    assert_eq!(text(&sides.old, 4).as_deref(), Some("d"), "the hunk's own line, rather than what was read again");
    assert_eq!(text(&sides.new, 8).as_deref(), Some("g"), "read again in part, and only the new part kept");
    assert_eq!(sides.new.as_in_the_file(7, 0..5).as_deref(), Some("\tf"), "with its tab as the file has it");
    assert_eq!((text(&sides.old, 1), sides.extent), (None, Extent::InPart(10)), "and nothing that wasn't read");
}

#[test]
fn a_line_cut_short_in_a_page_ends_in_an_ellipsis() {
    let page = [PageLine { text: "short".into(), cut: None }, PageLine { text: "lo".into(), cut: Some(70_000) }];

    let run = Run::from_page(&(8..10), &page, None);

    let texts: Vec<&str> = run.lines.iter().map(|line| line.text.as_ref()).collect();
    assert_eq!((run.numbers(), texts), (8..10, vec!["short", "lo…"]));
}

/// The text marked on each of lines `numbers` of `side`.
fn marked(side: &Side, numbers: Range<u32>) -> Vec<Vec<String>> {
    let line = |number| side.line(number).map(|line| line.text.to_string()).unwrap_or_default();
    numbers.map(|n| side.changed(n).iter().map(|range| line(n)[range.clone()].to_string()).collect()).collect()
}

#[test]
fn the_words_that_changed_are_marked_in_each_run_of_lines_that_took_another_s_place() {
    let changed = file("--- a/f\n+++ b/f\n@@ -1,4 +1,3 @@\n keep\n-# one two\n-# three\n+# one three\n keep\n");

    let sides = Sides::from_patch(&changed, None);

    assert_eq!(marked(&sides.old, 1..5), [vec![], vec!["two".to_string()], vec![], vec![]], "only on line 2");
    assert!(marked(&sides.new, 1..4).iter().all(Vec::is_empty), "`three` only moved");
    assert!(marked(&Sides::plain(&changed).old, 1..5).iter().all(Vec::is_empty), "none before they're made");
}

#[test]
fn marks_are_where_each_hunk_s_lines_are_drawn() {
    let patch =
        "--- a/f\n+++ b/f\n@@ -1,2 +1,3 @@\n keep\n+added\n keep\n@@ -10,2 +11,2 @@\n-\tx one\n+\tx two\n end\n";
    let changed = file(patch);

    let sides = Sides::from_patch(&changed, None);

    assert_eq!(marked(&sides.old, 10..11), [vec!["one".to_string()]], "after the tab, drawn as spaces");
    assert_eq!(marked(&sides.new, 11..12), [vec!["two".to_string()]], "a line further down on the new side");
}
