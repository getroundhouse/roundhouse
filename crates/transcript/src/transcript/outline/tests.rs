use std::collections::HashMap;
use std::sync::Arc;

use protocol::event::EventBody;
use protocol::model::{Proposal, ProposalId, Response, Rev, RevisionNumber, Round, Seq};
use protocol::prompt::{Prompt, PromptFrom};

use super::*;
use crate::list::Reading;
use crate::selection::Way;

/// Prompts drawn in these rows.
fn at_rows(rows: &[usize]) -> Vec<Line> {
    rows.iter().zip(1..).map(|(row, number)| prompt_at(*row, number)).collect()
}

/// The `seq` of the event in `row`, in a chat that holds every event.
fn seq_in(row: usize) -> Seq {
    Seq::new(u64::try_from(row).unwrap() + 1)
}

fn prompt_at(row: usize, number: usize) -> Line {
    prompt_saying(row, number, "", 0)
}

/// A prompt drawn in `row` that says `text` and carries `files` files.
fn prompt_saying(row: usize, number: usize, text: &str, files: u32) -> Line {
    Line { kind: Kind::Prompt { line: PromptLine::new(seq_in(row), text, files), number }, row }
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

/// The outline's lines, where the chat is read from, and which line is being read.
fn readings() -> Vec<(&'static str, Vec<Line>, Reading, usize)> {
    let three = at_rows(&[0, 10, 20]);
    let compacted = vec![prompt_at(0, 1), compaction_at(10), prompt_at(20, 2), compaction_at(30)];
    vec![
        ("the first, from its row", three.clone(), Reading::Scrolled(0), 0),
        ("the first, above the second", three.clone(), Reading::Scrolled(9), 0),
        ("a prompt at the bottom", three.clone(), Reading::Scrolled(10), 1),
        ("the last, below it", three.clone(), Reading::Scrolled(25), 2),
        ("above the first prompt, the first", at_rows(&[3, 10]), Reading::Scrolled(1), 0),
        ("a prompt kept at the bottom", three.clone(), Reading::Kept(10), 1),
        ("at the end, the last", three, Reading::End, 2),
        // A compaction is never the one being read.
        ("the prompt before a compaction", compacted.clone(), Reading::Scrolled(15), 0),
        ("the prompt before the last compaction", compacted.clone(), Reading::Scrolled(35), 2),
        ("at the end, the last prompt", compacted, Reading::End, 2),
        ("before any prompt, the first", vec![compaction_at(0), prompt_at(5, 1)], Reading::Scrolled(0), 1),
    ]
}

#[test]
fn the_prompt_being_read_is_the_last_at_or_above_the_bottom() {
    for (name, lines, reading, want) in readings() {
        assert_eq!(being_read(&lines, reading), want, "{name}");
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

/// What the outline says of a response's prompt, in the order a feed can bring them: the prompt's line before its
/// proposal's record says what was said to it, so the outline reads it again when only a record changed.
#[test]
fn a_responses_mark_follows_its_proposals_record() {
    let from = PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber::FIRST };
    let told = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("I'd like another round"), from };
    let log = crate::fixtures::log([crate::fixtures::event(Seq::new(1), told)]);
    let mut proposal = protocol::fixtures::proposal("p");
    proposal.rev = Rev::new(1);
    let mut known: HashMap<ProposalId, Arc<Proposal>> = HashMap::from([("p".into(), Arc::new(proposal.clone()))]);
    let said = |lines: &[Line]| match &lines[0].kind {
        Kind::Answer { said, .. } => said.clone(),
        Kind::Prompt { .. } | Kind::Compaction(_) => panic!("not drawn as a response"),
    };
    let mut outline = super::ticks::Outline::default();

    let before = said(&outline.read(&log, &known));
    proposal.current.response = Some(Response::AnotherRound(Round { note: "Smaller".into(), at_ms: 0 }));
    proposal.revisions = vec![proposal.current.summary()];
    proposal.rev = Rev::new(2);
    known.insert("p".into(), Arc::new(proposal));
    let after = said(&outline.read(&log, &known));

    assert_eq!((before.as_str(), after.as_str()), ("Answered Search", "Replied Search"));
}

/// Where a step goes, with the outline's lines in rows 0, 10, 20 and 30, the third a compaction, which a step stops at
/// as it does at a prompt.
#[test]
fn a_step_goes_from_the_line_at_the_bottom_or_the_end_to_the_line_beside_it() {
    let lines = vec![prompt_at(0, 1), prompt_at(10, 2), compaction_at(20), prompt_at(30, 3)];
    let steps = [
        ("back from the end, to the last", Reading::End, Way::Back, Some(Step::To(30))),
        ("on from the end, nowhere", Reading::End, Way::On, None),
        ("back from a line kept at the bottom, to the one before", Reading::Kept(30), Way::Back, Some(Step::To(20))),
        ("back from a compaction too", Reading::Kept(20), Way::Back, Some(Step::To(10))),
        ("on from a line kept at the bottom, to the one after", Reading::Kept(10), Way::On, Some(Step::To(20))),
        ("on from the last, to the end", Reading::Kept(30), Way::On, Some(Step::End)),
        ("back from the first, nowhere", Reading::Kept(0), Way::Back, None),
        ("back from below a line, to it first", Reading::Scrolled(15), Way::Back, Some(Step::To(10))),
        ("back from a line partly at the bottom, to it", Reading::Scrolled(10), Way::Back, Some(Step::To(10))),
        ("on from below a line, to the one after", Reading::Scrolled(15), Way::On, Some(Step::To(20))),
        ("on from below the last, to the end", Reading::Scrolled(35), Way::On, Some(Step::End)),
        ("back from a row kept at the bottom that isn't a line's", Reading::Kept(25), Way::Back, Some(Step::To(20))),
    ];

    for (name, reading, way, want) in steps {
        assert_eq!(step(&lines, reading, way), want, "{name}");
    }
}

/// Which prompts a step with Shift stops at: those in the run of rows loaded around where it goes from, and of those,
/// the ones with words, with the outline's lines in rows 0, 10, 15, 20 and 30, the third a compaction and the fourth
/// a prompt that only carries a file, rows 12 to 17 not loaded, and the list's tail in row 40.
#[test]
fn a_step_with_shift_stops_at_the_prompts_with_words_loaded_around_it() {
    let lines = vec![
        prompt_saying(0, 1, "Keep the ledger", 0),
        prompt_saying(10, 2, "And the rest", 0),
        compaction_at(15),
        prompt_saying(20, 3, "", 1),
        prompt_saying(30, 4, "Hold the train", 1),
    ];
    let (loaded, tail) = ([0..12, 18..40], 40);
    let cases = [
        ("from the first run, its prompts", &loaded[..], 5, vec![0, 10]),
        ("from the last, its prompt with words, not past the rows that aren't loaded", &loaded, 35, vec![30]),
        ("from a row that isn't loaded, none", &loaded, 14, vec![]),
        ("from the tail, the run that reaches it", &loaded, tail, vec![30]),
        ("from the tail with rows before it not loaded, none", &[0..12, 18..35], tail, vec![]),
    ];

    for (name, loaded, row, want) in cases {
        assert_eq!(selecting_stops(&lines, loaded, tail, row), want, "{name}");
    }
}
