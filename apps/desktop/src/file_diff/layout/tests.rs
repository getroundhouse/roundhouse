use super::*;
use crate::patch::parse;

/// The rows alone.
fn laid(shape: &Shape<'_>) -> Vec<Row> {
    rows(shape).rows
}

/// A file 40 lines long that changes line 30, with git's three lines either side.
fn changing_line_30() -> FilePatch {
    let lines: String =
        (27..=33).map(|line| if line == 30 { "-thirty\n+30\n".to_string() } else { format!(" {line}\n") }).collect();
    parse(&format!("--- a/f.txt\n+++ b/f.txt\n@@ -27,7 +27,7 @@\n{lines}")).unwrap().remove(0)
}

fn shape<'a>(file: &'a FilePatch, whole: Option<u32>, opened: &'a Opened, notes: &'a [Note]) -> Shape<'a> {
    let old = whole.map_or(Old::Hunks, Old::Whole);
    Shape { file, old, opened, commit: None, split: false, everything: false, notes }
}

/// What each row is, briefly: a fold's lines, or a line's numbers and how it changed.
fn briefly(rows: &[Row]) -> Vec<String> {
    let number = |line: &Option<u32>| line.map_or("-".to_string(), |line| line.to_string());
    rows.iter()
        .map(|row| match row {
            Row::Line { kind, old, new } => format!("{kind:?} {} {}", number(old), number(new)),
            Row::Pair { old, new, same } => {
                format!("{} {} {}", if *same { "same" } else { "pair" }, number(old), number(new))
            }
            Row::Fold(fold) => {
                format!("fold {}..{}{}", fold.old.start, fold.old.end, if fold.opens { "" } else { " shut" })
            }
            Row::Comment(id) => format!("comment {id}"),
            Row::Draft => "draft".to_string(),
        })
        .collect()
}

fn note(side: DiffSide, line: u32, id: &str) -> Note {
    Note { side, line, row: Row::Comment(CommentId::from(id)) }
}

#[test]
fn the_lines_git_left_out_fold_above_and_below_the_change() {
    let file = changing_line_30();
    let opened = Opened::default();

    let rows = laid(&shape(&file, Some(40), &opened, &[]));

    assert_eq!(briefly(&rows).first().map(String::as_str), Some("fold 1..27"));
    assert_eq!(
        briefly(&rows)[1..8],
        ["Same 27 27", "Same 28 28", "Same 29 29", "Removed 30 -", "Added - 30", "Same 31 31", "Same 32 32"]
    );
    assert_eq!(briefly(&rows)[8..], ["Same 33 33", "fold 34..41"]);
    let Row::Fold(top) = &rows[0] else { panic!() };
    let Row::Fold(end) = &rows[9] else { panic!() };
    assert_eq!((top.top, top.end, end.top, end.end), (true, false, false, true));
}

#[test]
fn without_the_file_as_it_was_the_lines_above_fold_shut_and_those_below_arent_known() {
    let file = changing_line_30();
    let opened = Opened::default();

    let rows = briefly(&laid(&shape(&file, None, &opened, &[])));

    assert_eq!(
        (rows.first().map(String::as_str), rows.last().map(String::as_str)),
        (Some("fold 1..27 shut"), Some("Same 33 33"))
    );
}

#[test]
fn what_was_opened_shows_between_what_stays_folded() {
    let file = changing_line_30();
    let mut opened = Opened::default();
    opened.open(None, 7..27);
    opened.open(None, 34..36);

    let rows = briefly(&laid(&shape(&file, Some(40), &opened, &[])));

    assert_eq!(rows[..3], ["fold 1..7", "Same 7 7", "Same 8 8"]);
    assert_eq!(rows[rows.len() - 3..], ["Same 34 34", "Same 35 35", "fold 36..41"]);
}

#[test]
fn a_line_with_a_comment_shows_even_where_it_would_fold() {
    let file = changing_line_30();
    let opened = Opened::default();
    let notes =
        [note(DiffSide::New, 10, "on-ten"), note(DiffSide::Old, 30, "on-thirty"), note(DiffSide::New, 99, "gone")];

    let rows = briefly(&laid(&shape(&file, Some(40), &opened, &notes)));

    assert_eq!(rows[..4], ["fold 1..10", "Same 10 10", "comment on-ten", "fold 11..27"]);
    let removed = rows.iter().position(|row| row == "Removed 30 -").unwrap();
    assert_eq!(rows[removed + 1], "comment on-thirty");
    assert_eq!(rows.last().map(String::as_str), Some("comment gone"), "its line isn't there now");
}

#[test]
fn side_by_side_each_removed_line_is_beside_the_one_that_took_its_place() {
    let file = parse("--- a/f\n+++ b/f\n@@ -1,4 +1,3 @@\n a\n-b\n-c\n+B\n d\n").unwrap().remove(0);
    let opened = Opened::default();

    let rows = laid(&Shape { split: true, ..shape(&file, Some(4), &opened, &[]) });

    assert_eq!(briefly(&rows), ["same 1 1", "pair 2 2", "pair 3 -", "same 4 3"]);
}

#[test]
fn the_whole_file_folds_nothing() {
    let file = changing_line_30();
    let opened = Opened::default();

    let rows = laid(&Shape { everything: true, ..shape(&file, Some(40), &opened, &[]) });

    assert!(!rows.iter().any(|row| matches!(row, Row::Fold(_))));
    assert_eq!(rows.len(), 41, "every line, and the one removed");
}

#[test]
fn a_row_laid_out_again_is_found_by_its_line_when_its_gone() {
    let file = changing_line_30();
    let (closed, mut opened) = (Opened::default(), Opened::default());
    let was = laid(&shape(&file, Some(40), &closed, &[]));
    opened.open(None, 34..41);

    let now = laid(&shape(&file, Some(40), &opened, &[]));

    assert_eq!(same_row(&was, &now, 1), Some(1), "line 27 is where it was");
    let at = same_row(&was, &now, 9).unwrap();
    assert_eq!(briefly(&now[at..=at]), ["Same 34 34"], "the fold at the end opened into its first line");
}

#[test]
fn lines_not_read_stay_folded_and_are_missing_until_they_are() {
    let file = changing_line_30();
    let mut opened = Opened::default();
    opened.open(None, 7..27);
    let notes = [note(DiffSide::New, 3, "c1")];
    let known = [5..10, 27..34];

    let old = Old::InPart { lines: 40, known: &known };
    let laid = rows(&Shape { old, ..shape(&file, None, &opened, &notes) });
    let everything = rows(&Shape { everything: true, old, ..shape(&file, None, &opened, &notes) });

    let shown = briefly(&laid.rows);
    let start = ["fold 1..7", "comment c1", "Same 7 7", "Same 8 8", "Same 9 9", "fold 10..27"];
    assert_eq!(shown[..6], start, "the note after the fold its line is in: {shown:?}");
    assert_eq!(laid.missing, [3..4, 10..27], "the line with a note, and what was opened but not read");
    assert_eq!(briefly(&everything.rows)[..3], ["fold 1..5", "comment c1", "Same 5 5"], "only what was read");
    assert_eq!(everything.missing, laid.missing, "and not every other line");
}
