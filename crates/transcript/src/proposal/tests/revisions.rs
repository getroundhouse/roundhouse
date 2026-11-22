//! Revisions the agent writes while the person reads or answers the one before: when one takes the page or waits, and
//! what's chosen that goes on to it, as `held` says.

use std::sync::Arc;

use gpui_kit::TestAppContext;
use protocol::model::{OptionLetter, OptionPlace, Proposal, ProposalOption, ProposalRevision, RevisionNumber};

use super::super::ProposalView;
use super::super::held::carried;
use super::support::{Hub, answered, answers, sent_anything, until_heard, view, view_on};

/// The fixture's proposal once the agent has revised it, with these options for its question, each under the letter
/// given. The fixture's own are A, On the hub, and B, In each client.
fn revised(options: &[(&str, &str)]) -> Arc<Proposal> {
    let first = protocol::fixtures::proposal("p");
    let mut current = ProposalRevision { number: RevisionNumber(2), ..first.current.clone() };
    let option = |(label, letter): &(&str, &str)| ProposalOption {
        label: (*label).into(),
        description: String::new(),
        body: String::new(),
        letter: OptionLetter::read(letter),
    };
    current.proposed.questions[0].options = options.iter().map(option).collect();
    Arc::new(Proposal { current, ..first })
}

/// The revision with the fixture's own options.
fn revised_as_it_was() -> Arc<Proposal> {
    revised(&[("On the hub", "A"), ("In each client", "B")])
}

/// A revision the agent writes while the person may be reading the one shown, or has chosen something in it, doesn't
/// take the page, and nothing is sent to the one it replaced, which the hub would refuse.
#[gpui_kit::test]
fn a_revision_is_held_back_while_the_tab_is_in_sight_or_holds_something_unsent(cx: &mut TestAppContext) {
    for in_sight in [true, false] {
        let (view, _hub) = view(cx);

        view.update(cx, |view, cx| {
            view.set_in_sight(in_sight, cx);
            if !in_sight {
                view.choose(0, Some(OptionPlace(1)), cx);
            }
            view.set_proposal(revised_as_it_was(), cx);
            view.choose(0, Some(OptionPlace(1)), cx);
            view.send(cx);
        });

        let (shown, held) = view.read_with(cx, |view, _| (view.proposal().current.number, view.held_back()));
        assert_eq!((shown, held), (RevisionNumber::FIRST, Some(RevisionNumber(2))), "in sight: {in_sight}");
        assert!(!sent_anything(&view, cx), "nothing is sent to a revision the agent replaced");
        assert!(view.read_with(cx, ProposalView::has_unsent), "what's chosen is kept");
    }
}

/// Behind another tab with nothing unsent, nobody is reading it, and once the one shown is responded to, there's
/// nothing to keep, so the newest shows at once, with nothing chosen.
#[gpui_kit::test]
fn a_revision_takes_the_page_at_once_when_nothing_would_be_lost(cx: &mut TestAppContext) {
    for responded_to in [false, true] {
        let (view, _) = view(cx);

        view.update(cx, |view, cx| {
            if responded_to {
                view.set_in_sight(true, cx);
                view.choose(0, Some(OptionPlace(1)), cx);
                view.set_proposal(answered(), cx);
            }
            view.set_proposal(revised_as_it_was(), cx);
        });

        let (shown, held) = view.read_with(cx, |view, _| (view.proposal().current.number, view.held_back()));
        assert_eq!((shown, held), (RevisionNumber(2), None), "responded to: {responded_to}");
        assert!(!view.read_with(cx, ProposalView::has_unsent), "what was chosen was sent, so it isn't chosen again");
    }
}

/// Showing the newest takes what was chosen along, by its letter, so it's sent as an answer to that revision.
#[gpui_kit::test]
fn showing_the_newest_keeps_what_was_chosen_under_its_letter(cx: &mut TestAppContext) {
    let (view, asked) = view(cx);

    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(1)), cx);
        view.set_proposal(revised(&[("In each client", "B"), ("On the hub", "A")]), cx);
        view.show_newest(cx);
        view.send(cx);
    });
    until_heard(1, &view, &asked, cx);

    let asked = answers(&asked);
    assert_eq!(asked.len(), 1);
    assert_eq!((asked[0].revision, asked[0].chosen.as_slice()), (RevisionNumber(2), &[OptionPlace(0)][..]));
    assert_eq!(view.read_with(cx, |view, _| view.held_back()), None);
}

/// A choice goes on only to the question at the same place with the same header, and only to the option under the
/// same letter, so one the agent dropped, or another question's, isn't answered for the person.
#[test]
fn what_was_chosen_goes_on_only_to_the_same_question_and_letter() {
    let first = protocol::fixtures::proposal("p").current.proposed;
    let chose_b = [Some(OptionPlace(1))];
    let second = |options: &[(&str, &str)]| revised(options).current.proposed.clone();
    let mut renamed = second(&[("On the hub", "A"), ("In each client", "B")]);
    renamed.questions[0].header = "Storage".into();
    let mut after_another = second(&[("On the hub", "A"), ("In each client", "B")]);
    after_another.questions.insert(0, renamed.questions[0].clone());

    let moved = carried(&chose_b, &first, &second(&[("In each client", "B"), ("On the hub", "A")]));
    let dropped = carried(&chose_b, &first, &second(&[("On the hub", "A"), ("In a file", "C")]));

    assert_eq!(moved, [Some(OptionPlace(0))], "B is first now");
    assert_eq!(dropped, [None], "B is gone");
    assert_eq!(carried(&chose_b, &first, &renamed), [None], "the question is another");
    assert_eq!(carried(&chose_b, &first, &after_another), [None, None], "the question has moved");
    assert_eq!(carried(&[], &first, &first), [None], "nothing was chosen");
}

/// An answer the hub kept just before the agent revised was to the revision shown, so the newest shows with nothing
/// chosen, rather than with what was already sent.
#[gpui_kit::test]
fn an_answer_kept_before_the_agent_revised_isnt_carried_on(cx: &mut TestAppContext) {
    let (view, asked) = view_on(Hub::Keeps, cx);

    view.update(cx, |view, cx| {
        view.set_in_sight(true, cx);
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
        view.set_proposal(revised_as_it_was(), cx);
        view.show_newest(cx);
    });
    let while_sending = view.read_with(cx, |view, _| view.proposal().current.number);
    until_heard(1, &view, &asked, cx);

    assert_eq!(while_sending, RevisionNumber::FIRST, "the page stays until the hub says whether it kept the answer");
    assert_eq!(asked.len(), 1);
    assert_eq!(view.read_with(cx, |view, _| view.proposal().current.number), RevisionNumber(2));
    assert!(!view.read_with(cx, ProposalView::has_unsent));
}
