use std::fmt::Write as _;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

fn only(patch: &str) -> FilePatch {
    let mut files = parse(patch).unwrap();
    assert_eq!(files.len(), 1, "{files:?}");
    files.remove(0)
}

/// Each line of a hunk as git writes it, with its sign.
fn signed(hunk: &Hunk) -> Vec<String> {
    let sign = |kind| match kind {
        LineKind::Same => ' ',
        LineKind::Removed => '-',
        LineKind::Added => '+',
    };
    hunk.lines().iter().map(|line| format!("{}{}", sign(line.kind), line.text)).collect()
}

#[test]
fn a_changed_file_has_its_hunks_in_order_with_where_each_starts_on_both_sides() {
    let file = only(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1a2b3c4..5d6e7f8 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n\
         @@ -1,3 +1,4 @@ fn main() {\n one\n+two\n three\n four\n@@ -10,2 +11,1 @@\n-ten\n eleven\n",
    );

    assert_eq!((file.path().as_ref(), file.old_path().map(AsRef::as_ref)), ("src/lib.rs", Some("src/lib.rs")));
    assert_eq!((file.status(), file.is_binary(), file.added(), file.removed()), (FileStatus::Modified, false, 1, 1));
    let [first, second] = file.hunks() else { panic!("{:?}", file.hunks()) };
    assert_eq!((first.old_start(), first.old_end(), first.new_start(), first.new_end()), (1, 4, 1, 5));
    assert_eq!(signed(first), [" one", "+two", " three", " four"]);
    assert_eq!((second.old_start(), second.old_end(), second.new_start(), second.new_end()), (10, 12, 11, 12));
}

#[test]
fn a_side_without_lines_starts_after_the_line_git_names() {
    let inserted = only("--- a/x\n+++ b/x\n@@ -5,0 +6,2 @@\n+six\n+seven\n");
    let added = only(
        "diff --git a/new.txt b/new.txt\nnew file mode 100644\nindex 0000000..e69de29\n--- /dev/null\n+++ b/new.txt\n\
         @@ -0,0 +1 @@\n+only\n",
    );
    let removed = only(
        "diff --git a/old.txt b/old.txt\ndeleted file mode 100644\n--- a/old.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\n",
    );

    assert_eq!((inserted.hunks()[0].old_start(), inserted.hunks()[0].new_start()), (6, 6));
    assert_eq!((added.status(), added.old_path(), added.path().as_ref()), (FileStatus::Added, None, "new.txt"));
    assert_eq!((added.hunks()[0].old_start(), added.hunks()[0].new_start()), (1, 1));
    assert_eq!((removed.status(), removed.new_path(), removed.path().as_ref()), (FileStatus::Removed, None, "old.txt"));
    assert_eq!((removed.hunks()[0].old_end(), removed.hunks()[0].new_start()), (3, 1));
}

#[test]
fn a_rename_takes_its_paths_from_its_own_lines_even_with_spaces_in_them() {
    let changed = only(
        "diff --git a/docs/old name.md b/docs/new name.md\nsimilarity index 90%\nrename from docs/old name.md\n\
         rename to docs/new name.md\n--- a/docs/old name.md\t\n+++ b/docs/new name.md\t\n@@ -1 +1 @@\n-a\n+b\n",
    );
    let moved = only("diff --git a/a.rs b/src/a.rs\nsimilarity index 100%\nrename from a.rs\nrename to src/a.rs\n");

    assert_eq!(changed.status(), FileStatus::Renamed);
    assert_eq!(
        (changed.old_path().unwrap().as_ref(), changed.path().as_ref()),
        ("docs/old name.md", "docs/new name.md")
    );
    assert_eq!(
        (moved.status(), moved.old_path().unwrap().as_ref(), moved.path().as_ref()),
        (FileStatus::Renamed, "a.rs", "src/a.rs")
    );
    assert!(moved.hunks().is_empty());
}

#[test]
fn a_copy_is_told_from_a_rename() {
    let copied = only("diff --git a/c.rs b/d.rs\nsimilarity index 80%\ncopy from c.rs\ncopy to d.rs\n");

    assert_eq!(
        (copied.status(), copied.old_path().unwrap().as_ref(), copied.path().as_ref()),
        (FileStatus::Copied, "c.rs", "d.rs")
    );
}

#[test]
fn a_file_that_isnt_text_has_its_paths_from_its_diff_line() {
    let files = parse(
        "diff --git a/shot one.png b/shot one.png\nindex 7c1d2e3..8f4a5b6 100644\n\
         Binary files a/shot one.png and b/shot one.png differ\ndiff --git a/new.png b/new.png\nnew file mode 100644\n\
         index 0000000..2b3c4d5\nGIT binary patch\nliteral 3\nKcmZQz00961\n\nliteral 0\nKcmV+b0RR6000031\n",
    )
    .unwrap();

    let found: Vec<_> = files.iter().map(|f| (f.path().as_ref(), f.status(), f.is_binary())).collect();
    assert_eq!(found, [("shot one.png", FileStatus::Modified, true), ("new.png", FileStatus::Added, true)]);
}

#[test]
fn a_quoted_path_is_read_as_git_escapes_it() {
    let file = only(
        "diff --git \"a/caf\\303\\251 \\\"menu\\\".txt\" \"b/caf\\303\\251 \\\"menu\\\".txt\"\n\
         --- \"a/caf\\303\\251 \\\"menu\\\".txt\"\n+++ \"b/caf\\303\\251 \\\"menu\\\".txt\"\n@@ -1 +1 @@\n-a\n+b\n",
    );

    assert_eq!(file.path().as_ref(), "café \"menu\".txt");
}

#[test]
fn prefixes_set_by_the_users_git_are_taken_off() {
    let mnemonic = only("diff --git c/src/a.rs w/src/a.rs\n--- c/src/a.rs\n+++ w/src/a.rs\n@@ -1 +1 @@\n-a\n+b\n");
    let none = only("diff --git src/a.rs src/a.rs\n--- src/a.rs\n+++ src/a.rs\n@@ -1 +1 @@\n-a\n+b\n");

    assert_eq!((mnemonic.path().as_ref(), none.path().as_ref()), ("src/a.rs", "src/a.rs"));
}

#[test]
fn a_line_that_reads_like_a_header_stays_a_line_of_its_hunk() {
    let file = only("--- a/q.sql\n+++ b/q.sql\n@@ -1,3 +1,2 @@\n--- a/note\n \n+++ b/note\n-x\n");

    assert_eq!(signed(&file.hunks()[0]), ["--- a/note", " ", "+++ b/note", "-x"]);
}

#[test]
fn an_empty_line_without_its_space_is_one_both_sides_have() {
    let file = only("--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n a\n\n-b\n+c\n");

    assert_eq!(signed(&file.hunks()[0]), [" a", " ", "-b", "+c"]);
}

#[test]
fn a_last_line_without_a_line_ending_is_marked() {
    let file = only("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-end\n\\ No newline at end of file\n+end\n");

    let marked: Vec<_> = file.hunks()[0].lines().iter().map(|line| (line.kind, line.no_newline)).collect();
    assert_eq!(marked, [(LineKind::Removed, true), (LineKind::Added, false)]);
}

#[test]
fn a_hunk_that_stops_short_of_its_counts_is_refused() {
    let short = parse("--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n a\n b\ndiff --git a/y b/y\n");
    let early = parse("@@ -1 +1 @@\n-a\n+b\n");
    let unordered = parse("--- a/x\n+++ b/x\n@@ -5 +5 @@\n-a\n+b\n@@ -2 +2 @@\n-c\n+d\n");

    assert_eq!(short.map_err(|e| (e.line, e.problem)), Err((6, Problem::ShortHunk)));
    assert_eq!(early.map_err(|e| e.problem), Err(Problem::HunkWithoutFile));
    assert_eq!(unordered.map_err(|e| e.problem), Err(Problem::Unordered));
}

#[test]
fn a_merges_diff_is_passed_over() {
    let files = parse("diff --cc a.rs\nindex 1,2..3\n@@@ -1,1 -1,1 +1,1 @@@\n- a\n -b\n++c\n").unwrap();

    assert!(files.is_empty());
}

/// What the new side is, from the old side and the file's hunks.
fn applied(old: &str, file: &FilePatch) -> String {
    let old: Vec<&str> = old.lines().collect();
    let (mut new, mut next) = (Vec::new(), 1_usize);
    for hunk in file.hunks() {
        let start = usize::try_from(hunk.old_start()).unwrap();
        new.extend(old[next - 1..start - 1].iter().map(ToString::to_string));
        new.extend(hunk.lines().iter().filter(|line| line.kind != LineKind::Removed).map(|line| line.text.to_string()));
        next = usize::try_from(hunk.old_end()).unwrap();
    }
    new.extend(old[next - 1..].iter().map(ToString::to_string));
    new.join("\n")
}

/// A patch from `old` to `new`, as git writes one, with `context` lines around each change. `similar`'s own can count
/// a hunk's lines wrong, so this counts them as it writes them.
fn unified(old: &str, new: &str, context: usize) -> String {
    let diff = similar::TextDiff::from_lines(old, new);
    let mut patch = String::from("--- a/f.txt\n+++ b/f.txt\n");
    for group in diff.grouped_ops(context) {
        let (mut lines, mut old_count, mut new_count) = (String::new(), 0, 0);
        for change in group.iter().flat_map(|op| diff.iter_changes(op)) {
            let sign = match change.tag() {
                similar::ChangeTag::Equal => ' ',
                similar::ChangeTag::Delete => '-',
                similar::ChangeTag::Insert => '+',
            };
            old_count += usize::from(sign != '+');
            new_count += usize::from(sign != '-');
            let _ = writeln!(lines, "{sign}{}", change.value().trim_end_matches('\n'));
            if change.missing_newline() {
                lines.push_str("\\ No newline at end of file\n");
            }
        }
        // A side with no lines names the line before where they'd be.
        let start = |start: usize, count: usize| start + usize::from(count > 0);
        let (old_start, new_start) = (group[0].old_range().start, group[0].new_range().start);
        let _ = write!(
            patch,
            "@@ -{},{old_count} +{},{new_count} @@\n{lines}",
            start(old_start, old_count),
            start(new_start, new_count)
        );
    }
    patch
}

fn text() -> impl Strategy<Value = String> {
    let line = prop::sample::select(vec!["a", "b", "c", "-- x", "++ y", "@@ z", "\\ w", ""]);
    (prop::collection::vec(line, 0..40), any::<bool>())
        .prop_map(|(lines, ended)| lines.join("\n") + if ended && !lines.is_empty() { "\n" } else { "" })
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// However two files differ, their diff read back turns the old one into the new.
    #[test]
    fn a_diff_read_back_turns_the_old_file_into_the_new(old in text(), new in text(), context in 0_usize..4) {
        let files = parse(&unified(&old, &new, context)).unwrap();

        let result = files.first().map_or_else(|| old.lines().collect::<Vec<_>>().join("\n"), |file| applied(&old, file));
        prop_assert_eq!(result, new.lines().collect::<Vec<_>>().join("\n"));
    }
}
