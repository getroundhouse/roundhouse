use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use client_core::stand_in_hub::Does;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use protocol::client::proposals::ProposalAnswer;
use protocol::model::{Choice, OptionLetter, OptionPlace, Proposal, ProposalRevision, Response, RevisionNumber};

use super::answering::Choosing;
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

/// What was chosen for one revision isn't sent as an answer to the next, whose options the person hasn't read.
#[gpui_kit::test]
fn what_was_chosen_is_let_go_when_the_agent_revises(cx: &mut TestAppContext) {
    let (view, asked) = view(cx);
    let first = protocol::fixtures::proposal("p");
    let revised =
        Proposal { current: ProposalRevision { number: RevisionNumber(2), ..first.current.clone() }, ..first };

    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(1)), cx);
        view.set_proposal(Arc::new(revised), cx);
        view.send(cx);
    });

    assert!(asked.borrow().is_empty());
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
