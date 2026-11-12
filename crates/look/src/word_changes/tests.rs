use super::*;

/// What's marked on each line of each side of a run.
fn marked<'a>(removed: &[&'a str], added: &[&'a str]) -> (Vec<Vec<&'a str>>, Vec<Vec<&'a str>>) {
    let (lost, gained) = compared(removed, added);
    (texts_of(removed, &lost), texts_of(added, &gained))
}

fn texts_of<'a>(lines: &[&'a str], marks: &[Vec<Range<usize>>]) -> Vec<Vec<&'a str>> {
    lines.iter().zip(marks).map(|(line, ranges)| ranges.iter().map(|range| &line[range.clone()]).collect()).collect()
}

/// Nothing marked on any of `removed` lines and `added` lines.
fn none(removed: usize, added: usize) -> (Vec<Vec<&'static str>>, Vec<Vec<&'static str>>) {
    (vec![Vec::new(); removed], vec![Vec::new(); added])
}

#[test]
fn only_the_words_that_changed_are_marked() {
    let (removed, added) = marked(&["let total = count + 1;"], &["let total = count * 2;"]);

    assert_eq!((removed, added), (vec![vec!["+ 1"]], vec![vec!["* 2"]]));
}

#[test]
fn a_change_to_code_marks_only_the_part_that_changed() {
    let (removed, added) = marked(&["        if aspect == Aspect::Stop {"], &["        if !aspect.may_pass() {"]);
    let (field, method) = marked(&["self.opened.clear();"], &["self.opened.drain();"]);

    assert_eq!((removed, added), (vec![vec!["== Aspect::Stop"]], vec![vec!["!", ".may_pass()"]]));
    assert_eq!((field, method), (vec![vec!["clear"]], vec![vec!["drain"]]));
}

#[test]
fn a_word_shared_by_chance_between_bigger_changes_is_marked_with_them() {
    let removed = ["Signals say whether the train may pass"];
    let added = ["Signals say how long the wait at the depot is"];

    let (removed, added) = marked(&removed, &added);

    assert_eq!(
        (removed, added),
        (vec![vec!["whether the train may pass"]], vec![vec!["how long the wait at the depot is"]])
    );
}

#[test]
fn what_s_kept_is_let_go_once_what_changed_around_it_outweighs_it() {
    // `of` goes first, and then `the`, which only the change it joined outweighs.
    let chance = marked(&["aaa the bb of cc"], &["xxx the yy of zz"]);
    // `the` outweighs what changed after it, so it stays.
    let kept = marked(&["aaaaaa the b"], &["xxxxxx the c"]);

    assert_eq!(chance, none(1, 1), "nothing's left in common, so nothing is picked out");
    assert_eq!(kept, (vec![vec!["aaaaaa", "b"]], vec![vec!["xxxxxx", "c"]]));
}

#[test]
fn words_that_moved_to_another_line_are_not_marked() {
    let removed = ["# Builds the app from each merge and publishes it", "# as an update."];
    let added = ["# Builds each merge and publishes it as", "# an update."];

    let (removed, added) = marked(&removed, &added);

    assert_eq!(removed, [vec!["the app from"], vec![]]);
    assert_eq!(added, [Vec::<&str>::new(), vec![]], "nor the space where a line no longer breaks");
}

#[test]
fn a_line_s_indentation_and_comment_marks_are_not_compared() {
    let (removed, added) = marked(&["    // one", "    //", "    // two"], &["    // one two"]);

    assert_eq!((removed, added), none(3, 1), "no line's `//` changed");
}

#[test]
fn indentation_and_space_at_the_end_that_changed_are_marked() {
    let (removed, added) = marked(&["foo();  "], &["    foo();"]);

    assert_eq!((removed, added), (vec![vec!["  "]], vec![vec!["    "]]));
}

#[test]
fn indentation_is_not_marked_on_lines_that_were_joined_or_wrapped_again() {
    let joined =
        marked(&["    let x = foo(", "        alpha,", "        beta,", "    );"], &["    let x = foo(alpha, beta);"]);
    let wrapped = marked(&["- foo bar", "  baz qux"], &["- foo", "  bar baz qux"]);

    assert_eq!(joined, (vec![vec![], vec![], vec![","], vec![]], vec![vec![]]), "only the comma after `beta` went");
    assert_eq!(wrapped, none(2, 2));
}

#[test]
fn runs_with_nothing_in_common_or_too_long_have_nothing_marked() {
    assert_eq!(marked(&["one two"], &["three four"]), none(1, 1));
    assert_eq!(marked(&["one two "], &["three four "]), none(1, 1), "space at the end is all they share");
    let (long, longer) = ("same a ".repeat(200), "same b ".repeat(200));
    assert_eq!(marked(&[&long], &[&longer]), none(1, 1), "too long to compare");
}

#[test]
fn a_word_with_wide_characters_is_marked_whole() {
    let (removed, added) = marked(&["héllo wörld"], &["héllo wérld"]);

    assert_eq!((removed, added), (vec![vec!["wörld"]], vec![vec!["wérld"]]));
}

#[test]
fn each_run_of_removed_lines_is_compared_with_the_added_lines_after_it() {
    let lines = [
        (LineChange::Same, "keep"),
        (LineChange::Removed, "one a"),
        (LineChange::Added, "one b"),
        (LineChange::Removed, "two c"),
        (LineChange::Removed, "two d"),
        (LineChange::Added, "two e"),
        (LineChange::Same, "keep"),
        (LineChange::Removed, "gone"),
        (LineChange::Same, "keep"),
        (LineChange::Added, "new"),
    ];

    let marks = word_changes(&lines);

    let marked: Vec<Vec<&str>> =
        lines.iter().zip(&marks).map(|((_, text), ranges)| ranges.iter().map(|r| &text[r.clone()]).collect()).collect();
    let expected: [&[&str]; 10] = [&[], &["a"], &["b"], &["c"], &["two d"], &["e"], &[], &[], &[], &[]];
    assert_eq!(marked, expected);
}
