use super::*;

fn option(label: &str) -> ProposalOption {
    ProposalOption { label: label.into(), description: String::new(), body: String::new() }
}

fn question(options: usize) -> ProposalQuestion {
    ProposalQuestion {
        header: "Index".into(),
        text: "Where does the index live?".into(),
        details: String::new(),
        options: (0..options).map(|at| option(&format!("Option {at}"))).collect(),
        recommended: None,
    }
}

fn proposed(questions: usize) -> Proposed {
    Proposed {
        title: "Search".into(),
        context: String::new(),
        questions: (0..questions).map(|_| question(2)).collect(),
    }
}

#[test]
fn a_proposal_has_one_to_four_questions_of_two_to_four_options() {
    assert_eq!(proposed(1).check(), Ok(()));
    assert_eq!(proposed(MAX_QUESTIONS).check(), Ok(()));
    assert_eq!(proposed(0).check(), Err(ProposalProblem::Questions(0)));
    assert_eq!(proposed(5).check(), Err(ProposalProblem::Questions(5)));
    for (options, fits) in [(1, false), (2, true), (4, true), (5, false)] {
        let checked = Proposed { questions: vec![question(options)], ..proposed(1) }.check();

        assert_eq!(checked.is_ok(), fits, "{options} options");
    }
}

#[test]
fn what_a_proposal_lacks_is_said_by_the_question_and_option_its_in() {
    let untitled = ProposalQuestion { options: vec![option("Here"), option(" ")], ..question(2) };
    let unasked = ProposalQuestion { text: " ".into(), ..question(2) };
    let long = ProposalQuestion { header: "h".repeat(MAX_HEADER_CHARS + 1), ..question(2) };
    let recommends_c = ProposalQuestion {
        recommended: Some(Recommendation { option: OptionPlace(2), why: String::new() }),
        ..question(2)
    };

    let check = |second: ProposalQuestion| Proposed { questions: vec![question(2), second], ..proposed(1) }.check();

    assert_eq!(check(untitled), Err(ProposalProblem::Unlabelled { question: 2, letter: 'B' }));
    assert_eq!(check(unasked), Err(ProposalProblem::Unasked { question: 2 }));
    assert_eq!(check(long), Err(ProposalProblem::LongHeader { question: 2, chars: MAX_HEADER_CHARS + 1 }));
    assert_eq!(check(recommends_c), Err(ProposalProblem::NoSuchOption { question: 2, letter: 'C' }));
    assert_eq!(Proposed { title: String::new(), ..proposed(1) }.check(), Err(ProposalProblem::NoTitle));
}

#[test]
fn a_proposal_holds_at_most_so_much_text() {
    let long = Proposed { context: "x".repeat(MAX_PROPOSAL_BYTES), ..proposed(1) };

    assert!(matches!(long.check(), Err(ProposalProblem::TooLong(_))));
}

#[test]
fn an_option_is_named_by_a_letter_from_a() {
    assert_eq!((OptionPlace(0).letter(), OptionPlace(3).letter()), ('A', 'D'));
    assert_eq!(OptionPlace::of_letter("b"), Some(OptionPlace(1)));
    assert_eq!(OptionPlace::of_letter(" D "), Some(OptionPlace(3)));
    for not_one in ["", "AB", "1", "é"] {
        assert_eq!(OptionPlace::of_letter(not_one), None, "{not_one:?}");
    }
}

#[test]
fn an_answer_chooses_one_of_each_questions_options() {
    let two = proposed(2);
    assert_eq!(two.check_answer(&[OptionPlace(0), OptionPlace(1)], ""), Ok(()));
    assert_eq!(two.check_answer(&[OptionPlace(0)], ""), Err(AnswerProblem::Unchosen { questions: 2 }));
    assert_eq!(
        two.check_answer(&[OptionPlace(0), OptionPlace(2)], ""),
        Err(AnswerProblem::NoSuchOption { question: 2, letter: 'C' })
    );
    let long = "x".repeat(MAX_NOTE_BYTES + 1);
    assert_eq!(two.check_answer(&[OptionPlace(0), OptionPlace(0)], &long), Err(AnswerProblem::LongNote(long.len())));
}

#[test]
fn what_was_chosen_names_each_option_by_its_question_when_there_are_several() {
    let answered = |questions: usize, chosen: Vec<OptionPlace>| ProposalRevision {
        number: RevisionNumber::FIRST,
        call: None,
        proposed: Proposed { questions: (0..questions).map(|_| question(3)).collect(), ..proposed(1) },
        made_at_ms: 0,
        response: Some(Response::Chose(Choice { chosen, note: String::new(), at_ms: 0 })),
        images: Vec::new(),
    };

    assert_eq!(answered(1, vec![OptionPlace(1)]).chosen_in_words().as_deref(), Some("B, Option 1"));
    assert_eq!(
        answered(2, vec![OptionPlace(0), OptionPlace(2)]).chosen_in_words().as_deref(),
        Some("Index: A, Option 0; Index: C, Option 2")
    );
    assert_eq!(ProposalRevision { response: None, ..answered(1, vec![]) }.chosen_in_words(), None);
}
