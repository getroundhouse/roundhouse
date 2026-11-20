use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use client_core::stand_in_hub::Does;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use protocol::client::proposals::ProposalAnswer;
use protocol::model::{
    Choice, OptionLetter, OptionPlace, Proposal, ProposalOption, ProposalRevision, Response, RevisionNumber,
};

use super::answering::Choosing;
use super::held::carried;
use super::{ProposalEvent, ProposalRequest, ProposalView};

/// The view, and everything it asks the app to send, as it asks. Its hub answers nothing, since the view only reads
/// images from it, and the proposal points to none.
fn view(cx: &mut TestAppContext) -> (Entity<ProposalView>, Rc<RefCell<Vec<ProposalRequest>>>) {
    let dir = tempfile::tempdir().unwrap();
    let client = client_core::stand_in_hub::connect(&dir.path().join("hub.sock"), |_| Does::Nothing);
    let proposal = Arc::new(protocol::fixtures::proposal("p"));
    let view = cx.update(|cx| cx.new(|cx| ProposalView::new(proposal, client, cx)));
    let asked = Rc::new(RefCell::new(Vec::new()));
    let heard = asked.clone();
    cx.update(|cx| {
        cx.subscribe(&view, move |_, event: &ProposalEvent, _| {
            if let ProposalEvent::Send(request) = event {
                heard.borrow_mut().push(request.clone());
            }
        })
        .detach();
    });
    (view, asked)
}

/// The answers among what the view asked to send.
fn answers(asked: &RefCell<Vec<ProposalRequest>>) -> Vec<ProposalAnswer> {
    let answer = |request: &ProposalRequest| match request {
        ProposalRequest::Answer(answer) => Some(answer.clone()),
        ProposalRequest::AnotherRound(_) => None,
    };
    asked.borrow().iter().filter_map(answer).collect()
}

fn responded(response: Response) -> Arc<Proposal> {
    let proposal = protocol::fixtures::proposal("p");
    Arc::new(Proposal { current: ProposalRevision { response: Some(response), ..proposal.current }, ..proposal })
}

fn answered() -> Arc<Proposal> {
    responded(Response::Chose(Choice { chosen: vec![OptionPlace(1)], note: String::new(), at_ms: 0 }))
}

/// One that couldn't be sent goes again as the same request, so if the hub kept it after all, it's kept once.
#[gpui_kit::test]
fn an_answer_sent_again_after_a_failure_is_the_same_request(cx: &mut TestAppContext) {
    let (view, asked) = view(cx);

    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
        view.sent(Err("the hub went away".into()), cx);
        view.send(cx);
    });

    let asked = answers(&asked);
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0].chosen, [OptionPlace(1)]);
    assert_eq!(asked[1], asked[0], "the same answer, by the same request");
}

/// One chosen otherwise after a failure is a new request, so a hub that kept the first doesn't take the change for it.
#[gpui_kit::test]
fn an_answer_changed_after_a_failure_is_a_new_request(cx: &mut TestAppContext) {
    let (view, asked) = view(cx);

    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
        view.sent(Err("the hub went away".into()), cx);
        view.choose(0, Some(OptionPlace(0)), cx);
        view.send(cx);
    });

    let asked = answers(&asked);
    assert_eq!(asked[1].chosen, [OptionPlace(0)]);
    assert_ne!(asked[1].client_request_id, asked[0].client_request_id);
}

/// Once it's answered, here or in another client, or responded to in a way only a newer hub knows, there's nothing to
/// send.
#[gpui_kit::test]
fn nothing_is_sent_once_its_responded_to(cx: &mut TestAppContext) {
    for response in [answered(), responded(Response::Unknown)] {
        let (view, asked) = view(cx);

        view.update(cx, |view, cx| {
            view.choose(0, Some(OptionPlace(0)), cx);
            view.set_proposal(response, cx);
            view.send(cx);
        });

        assert!(asked.borrow().is_empty());
    }
}

/// Its lines say it was responded to, as the tab does, rather than that it waits, though nothing was chosen.
#[test]
fn a_response_only_a_newer_hub_knows_is_drawn_as_responded() {
    let newer = responded(Response::Unknown);

    let line = crate::proposal_response_line(Some(&newer), RevisionNumber::FIRST);
    assert_eq!((line.verb, line.title.as_deref(), line.said.as_str()), ("Answered", Some("Search"), "Responded"));
}

/// Another round is a reply, rather than an answer, which says only that the note went with it, since it always does.
#[test]
fn another_round_is_drawn_as_a_reply_with_a_note() {
    let round = protocol::model::Round { note: "Could the board own it?".into(), at_ms: 0 };
    let asked = responded(Response::AnotherRound(round));

    let line = crate::proposal_response_line(Some(&asked), RevisionNumber::FIRST);

    assert_eq!((line.verb, line.said.as_str()), ("Replied", "Another round, with a note"));
}

#[gpui_kit::test]
fn an_answer_is_unsent_while_anything_is_chosen_and_not_once_its_answered(cx: &mut TestAppContext) {
    let (view, _) = view(cx);

    let unsent = |view: &Entity<ProposalView>, cx: &mut TestAppContext| view.read_with(cx, ProposalView::has_unsent);
    assert!(!unsent(&view, cx));
    view.update(cx, |view, cx| view.choose(0, Some(OptionPlace(0)), cx));
    assert!(unsent(&view, cx));
    view.update(cx, |view, cx| view.set_proposal(answered(), cx));
    assert!(!unsent(&view, cx));
}

/// Send pressed again while the answer is on its way sends nothing more, so the hub isn't asked twice at once.
#[gpui_kit::test]
fn an_answer_on_its_way_isnt_sent_again(cx: &mut TestAppContext) {
    let (view, asked) = view(cx);

    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
        view.send(cx);
    });

    assert_eq!(asked.borrow().len(), 1);
}

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
        let (view, asked) = view(cx);

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
        assert!(asked.borrow().is_empty(), "nothing is sent to a revision the agent replaced");
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
    let (view, asked) = view(cx);

    view.update(cx, |view, cx| {
        view.set_in_sight(true, cx);
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
        view.set_proposal(revised_as_it_was(), cx);
        view.show_newest(cx);
    });
    let while_sending = view.read_with(cx, |view, _| view.proposal().current.number);
    view.update(cx, |view, cx| view.sent(Ok(()), cx));

    assert_eq!(while_sending, RevisionNumber::FIRST, "the page stays until the hub says whether it kept the answer");
    assert_eq!(asked.borrow().len(), 1);
    assert_eq!(view.read_with(cx, |view, _| view.proposal().current.number), RevisionNumber(2));
    assert!(!view.read_with(cx, ProposalView::has_unsent));
}

/// Once the person's asked for another round, or responded in a way only a newer hub knows, the cards offer nothing to
/// choose, as the bar does.
#[gpui_kit::test]
fn a_response_other_than_a_choice_leaves_nothing_to_choose(cx: &mut TestAppContext) {
    let (view, _) = view(cx);
    let round = protocol::model::Round { note: "Could the board own it?".into(), at_ms: 0 };
    let (asked, newer) = (responded(Response::AnotherRound(round)), responded(Response::Unknown));
    let waiting = protocol::fixtures::proposal("p");

    let buttons = cx.update(|_| {
        [waiting.current.standing(), asked.current.standing(), newer.current.standing()].map(|standing| {
            let choosing = Choosing { this: view.downgrade(), chosen: &[], standing };
            choosing.button(0, OptionPlace(0), OptionLetter::read("A").unwrap()).is_some()
        })
    });

    assert_eq!(buttons, [true, false, false], "Choose while it waits, and nothing once it's responded to");
}
