use protocol::model::Seq;

use super::*;

/// Prompts drawn in these rows.
fn at_rows(rows: &[usize]) -> Vec<Line> {
    rows.iter().zip(1..).map(|(row, number)| prompt_at(*row, number)).collect()
}

/// The `seq` of the event in `row`, in a chat that holds every event.
fn seq_in(row: usize) -> Seq {
    Seq::new(u64::try_from(row).unwrap() + 1)
}

fn prompt_at(row: usize, number: usize) -> Line {
    Line { kind: Kind::Prompt { line: PromptLine::new(seq_in(row), "", 0), number }, row }
}

fn compaction_at(row: usize) -> Line {
    let line = CompactionLine {
        seq: seq_in(row),
        place: protocol::model::Place::default(),
        by: protocol::event::CompactedBy::Agent,
        before_tokens: None,
        after_tokens: None,
    };
    Line { kind: Kind::Compaction(line), row }
}

/// The outline's lines, the row at the bottom of the chat's window, whether the chat is at its end, and which line is
/// being read.
fn readings() -> Vec<(&'static str, Vec<Line>, usize, bool, usize)> {
    let three = at_rows(&[0, 10, 20]);
    let compacted = vec![prompt_at(0, 1), compaction_at(10), prompt_at(20, 2), compaction_at(30)];
    vec![
        ("the first, from its row", three.clone(), 0, false, 0),
        ("the first, above the second", three.clone(), 9, false, 0),
        ("a prompt at the bottom", three.clone(), 10, false, 1),
        ("the last, below it", three.clone(), 25, false, 2),
        ("above the first prompt, the first", at_rows(&[3, 10]), 1, false, 0),
        ("at the end, the last, whatever the bottom row", three, 12, true, 2),
        // A compaction is never the one being read.
        ("the prompt before a compaction", compacted.clone(), 15, false, 0),
        ("the prompt before the last compaction", compacted.clone(), 35, false, 2),
        ("at the end, the last prompt", compacted, 35, true, 2),
        ("before any prompt, the first", vec![compaction_at(0), prompt_at(5, 1)], 0, false, 1),
    ]
}

#[test]
fn the_prompt_being_read_is_the_last_at_or_above_the_bottom() {
    for (name, lines, bottom, at_end, want) in readings() {
        assert_eq!(being_read(&lines, bottom, at_end), want, "{name}");
    }
}

#[test]
fn every_tick_shows_while_they_fit() {
    assert_eq!(shown(MOST_TICKS, MOST_TICKS - 1), 0..MOST_TICKS);
}

#[test]
fn more_than_fit_show_around_the_one_being_read_and_stop_at_the_ends() {
    let count = MOST_TICKS * 3;

    assert_eq!(shown(count, 0), 0..MOST_TICKS);
    assert_eq!(shown(count, count - 1), count - MOST_TICKS..count);
    let middle = shown(count, count / 2);
    assert_eq!(middle.len(), MOST_TICKS);
    assert!(middle.contains(&(count / 2)) && middle.start > 0 && middle.end < count);
}

#[test]
fn a_tick_at_an_end_with_more_past_it_is_faded() {
    let count = MOST_TICKS * 3;
    let at = count / 2;
    let shown = shown(count, at);
    let looks: Vec<Tick> = shown.clone().map(|ix| look(ix, at, &shown, count)).collect();

    assert_eq!(looks.first(), Some(&Tick::Faded));
    assert_eq!(looks.last(), Some(&Tick::Faded));
    assert_eq!(looks.iter().filter(|look| **look == Tick::BeingRead).count(), 1);
    assert!(looks[1..looks.len() - 1].iter().all(|look| *look != Tick::Faded));
}

#[test]
fn a_tick_at_the_chats_own_end_is_not_faded() {
    let count = MOST_TICKS * 3;
    let first = shown(count, 0);
    let last = shown(count, count - 1);

    assert_eq!(look(1, 0, &first, count), Tick::Plain, "nothing is before the first");
    assert_eq!(look(MOST_TICKS - 1, 0, &first, count), Tick::Faded, "more are after it");
    assert_eq!(look(count - 2, count - 1, &last, count), Tick::Plain, "nothing is after the last");
    assert_eq!(look(0, 3, &(0..5), 5), Tick::Plain, "every one shows");
}
