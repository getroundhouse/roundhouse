use super::*;

/// The prompt `line`, in its own row in a chat that holds every event.
fn prompt(line: PromptLine) -> Line {
    Line { row: usize::try_from(line.seq()).unwrap() - 1, kind: Kind::Prompt { line, number: 1 } }
}

fn lines(seqs: &[u64]) -> Vec<Line> {
    seqs.iter().map(|seq| prompt(PromptLine::new(*seq, "a prompt", 0))).collect()
}

#[test]
fn a_prompt_recorded_joins_the_end_and_nothing_before_it_is_measured_again() {
    assert_eq!(changed(&lines(&[1, 5]), &lines(&[1, 5, 9])), (2..2, 1));
}

#[test]
fn prompts_the_hub_lists_before_those_the_chat_has_go_in_before_them() {
    assert_eq!(changed(&lines(&[700, 900]), &lines(&[3, 40, 700, 900])), (0..0, 2));
    assert_eq!(changed(&lines(&[3, 900]), &lines(&[3, 40, 700, 900])), (1..1, 2), "or between them");
}

#[test]
fn a_prompt_whose_line_changed_is_measured_again_where_it_is() {
    let was = lines(&[1, 5, 9]);
    let mut now = was.clone();
    now[1] =
        prompt(PromptLine::new(5, "a prompt, and a much longer one, which may take two lines where it took one", 0));

    assert_eq!(changed(&was, &now), (1..2, 1));
}

#[test]
fn the_same_prompts_change_nothing() {
    assert_eq!(changed(&lines(&[1, 5]), &lines(&[1, 5])), (2..2, 0));
    assert_eq!(changed(&[], &[]), (0..0, 0));
}

#[test]
fn rows_alike_at_the_edge_of_what_changed_are_counted_once() {
    let same = |count| vec![prompt(PromptLine::new(1, "a prompt", 0)); count];

    assert_eq!(changed(&same(1), &same(2)), (1..1, 1));
    assert_eq!(changed(&same(2), &same(1)), (1..2, 0));
}

#[test]
fn a_prompt_with_only_files_says_how_many() {
    assert_eq!(label(&PromptLine::new(1, " \n", 1)).as_ref(), "1 file");
    assert_eq!(label(&PromptLine::new(1, "", 3)).as_ref(), "3 files");
    assert_eq!(label(&PromptLine::new(1, "Fix it", 3)).as_ref(), "Fix it", "the text, when there is some");
}
