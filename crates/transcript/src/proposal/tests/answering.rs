//! What the view sends the hub, and how it hears back: an answer, again after a failure, once only, and on the
//! connection given last, and what the cards and the chat's lines say once it's responded to.

use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use protocol::model::{OptionLetter, OptionPlace, Response, RevisionNumber};

use super::super::ProposalView;
use super::super::answering::Choosing;
use super::support::{Hub, answered, answers, responded, sent_anything, until_heard, view, view_on};

/// One that couldn't be sent goes again as the same request, so if the hub kept it after all, it's kept once.
#[gpui_kit::test]
fn an_answer_sent_again_after_a_failure_is_the_same_request(cx: &mut TestAppContext) {
    let (view, asked) = view(cx);

    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
    });
    until_heard(1, &view, &asked, cx);
    view.update(cx, ProposalView::send);
    until_heard(2, &view, &asked, cx);

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
    });
    until_heard(1, &view, &asked, cx);
    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(0)), cx);
        view.send(cx);
    });
    until_heard(2, &view, &asked, cx);

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

        assert!(!sent_anything(&view, cx));
        assert_eq!(asked.len(), 0);
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
    let (view, _hub) = view(cx);

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
    let (view, asked) = view_on(Hub::Waits, cx);

    view.update(cx, |view, cx| {
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
        view.send(cx);
    });

    waiting::thread::until(Duration::from_secs(10), "the hub to hear the answer", || asked.len() > 0);
    // The client sends requests side by side, so a second would reach the hub while the first waits.
    waiting::thread::for_what_mustnt_happen(Duration::from_millis(200));
    assert_eq!(asked.len(), 1);
}

/// Once the person's asked for another round, or responded in a way only a newer hub knows, the cards offer nothing to
/// choose, as the bar does.
#[gpui_kit::test]
fn a_response_other_than_a_choice_leaves_nothing_to_choose(cx: &mut TestAppContext) {
    let (view, _hub) = view(cx);
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
