use super::*;

/// Prompts drawn in these rows.
fn at_rows(rows: &[usize]) -> Vec<Line> {
    rows.iter().zip(1..).map(|(row, number)| prompt_at(*row, number)).collect()
}

fn prompt_at(row: usize, number: usize) -> Line {
    Line { kind: Kind::Prompt { line: PromptLine::new(row as u64 + 1, "", 0), number }, row }
}

fn compaction_at(row: usize) -> Line {
    let line = CompactionLine {
        seq: row as u64 + 1,
        place: protocol::model::Place::default(),
        by: protocol::event::CompactedBy::Agent,
        before_tokens: None,
        after_tokens: None,
    };
    Line { kind: Kind::Compaction(line), row }
}

#[test]
fn a_compaction_is_never_the_one_being_read() {
    let lines = [prompt_at(0, 1), compaction_at(10), prompt_at(20, 2), compaction_at(30)];

    assert_eq!(being_read(&lines, 15, false), 0, "the prompt before it is");
    assert_eq!(being_read(&lines, 35, false), 2);
    assert_eq!(being_read(&lines, 35, true), 2, "and at the end, the last prompt is");
    assert_eq!(being_read(&[compaction_at(0), prompt_at(5, 1)], 0, false), 1, "and before any, the first");
}

#[test]
fn the_prompt_being_read_is_the_last_at_or_above_the_bottom() {
    let lines = at_rows(&[0, 10, 20]);

    assert_eq!(being_read(&lines, 0, false), 0);
    assert_eq!(being_read(&lines, 9, false), 0);
    assert_eq!(being_read(&lines, 10, false), 1, "a prompt at the bottom is the one being read");
    assert_eq!(being_read(&lines, 25, false), 2);
}

#[test]
fn above_the_first_prompt_the_first_is_being_read() {
    assert_eq!(being_read(&at_rows(&[3, 10]), 1, false), 0);
}

#[test]
fn at_the_end_the_last_prompt_is_being_read_whatever_the_bottom_row() {
    assert_eq!(being_read(&at_rows(&[0, 10, 20]), 12, true), 2);
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
