use std::ops::Range;

use super::*;
use protocol::model::Seq;

/// The prompt `line`, in its own row in a chat no subagent has worked in, where each event's place is its `seq`.
fn prompt(line: PromptLine) -> Line {
    Line { row: usize::try_from(line.seq().get()).unwrap() - 1, kind: Kind::Prompt { line, number: 1 } }
}

fn lines(seqs: &[u64]) -> Vec<Line> {
    seqs.iter().map(|seq| prompt(PromptLine::new(Seq::new(*seq), "a prompt", 0))).collect()
}

/// Which rows of the prompts before a change are measured again, as how many of those after.
type Changed = (Range<usize>, usize);

/// The prompts before and after a change, and what of them changed.
fn changes() -> Vec<(&'static str, Vec<Line>, Vec<Line>, Changed)> {
    let mut longer = lines(&[1, 5, 9]);
    longer[1] = prompt(PromptLine::new(
        Seq::new(5),
        "a prompt, and a much longer one, which may take two lines where it took one",
        0,
    ));
    let same = |count| vec![prompt(PromptLine::new(Seq::new(1), "a prompt", 0)); count];
    vec![
        // Nothing before it is measured again.
        ("a prompt recorded joins the end", lines(&[1, 5]), lines(&[1, 5, 9]), (2..2, 1)),
        ("prompts the hub lists go in before the chat's", lines(&[700, 900]), lines(&[3, 40, 700, 900]), (0..0, 2)),
        ("or between them", lines(&[3, 900]), lines(&[3, 40, 700, 900]), (1..1, 2)),
        ("a prompt whose line changed is measured again where it is", lines(&[1, 5, 9]), longer, (1..2, 1)),
        ("the same prompts change nothing", lines(&[1, 5]), lines(&[1, 5]), (2..2, 0)),
        ("nor do none", Vec::new(), Vec::new(), (0..0, 0)),
        // Rows alike at the edge of what changed are counted once.
        ("one more alike", same(1), same(2), (1..1, 1)),
        ("one fewer alike", same(2), same(1), (1..2, 0)),
    ]
}

#[test]
fn only_the_prompts_between_the_start_and_the_end_the_two_share_are_measured_again() {
    for (name, was, now, want) in changes() {
        assert_eq!(changed(&was, &now), want, "{name}");
    }
}

#[test]
fn a_prompt_with_only_files_says_how_many() {
    assert_eq!(label(&PromptLine::new(Seq::new(1), " \n", 1)).as_ref(), "1 file");
    assert_eq!(label(&PromptLine::new(Seq::new(1), "", 3)).as_ref(), "3 files");
    assert_eq!(label(&PromptLine::new(Seq::new(1), "Fix it", 3)).as_ref(), "Fix it", "the text, when there is some");
}
