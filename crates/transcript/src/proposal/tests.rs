use std::sync::Arc;
use std::time::Duration;

use client_core::stand_in_hub::Does;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use parking_lot::Mutex;
use protocol::client::proposals::ProposalAnswer;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::model::{
    Choice, OptionLetter, OptionPlace, Proposal, ProposalOption, ProposalRevision, Response, RevisionNumber,
};

use super::ProposalView;
use super::answering::{Choosing, ProposalRequest, Sending};
use super::held::carried;

/// What the hub heard the view send, as it heard it, and the folder its socket is in, which goes once the test lets go
/// of this.
#[derive(Clone)]
struct Heard {
    requests: Arc<Mutex<Vec<ProposalRequest>>>,
    _dir: Arc<tempfile::TempDir>,
}

impl Heard {
    fn len(&self) -> usize {
        self.requests.lock().len()
    }
}

/// How the hub stood in for answers what the view sends.
#[derive(Clone, Copy)]
enum Hub {
    /// That it couldn't keep it, as a hub that went away mid-request would.
    Refuses,
    /// Nothing, so what was sent stays on its way.
    Waits,
    /// That it kept it.
    Keeps,
}

/// The view, on a hub that answers what it sends as `hub` says, and everything that hub hears the view send.
fn view_on(hub: Hub, cx: &mut TestAppContext) -> (Entity<ProposalView>, Heard) {
    let (client, heard) = stand_in(hub, cx);
    let proposal = Arc::new(protocol::fixtures::proposal("p"));
    let view = cx.update(|cx| cx.new(|cx| ProposalView::new(proposal, client, cx)));
    (view, heard)
}

/// A connection to a hub that answers what the view sends as `hub` says, and everything that hub hears.
fn stand_in(hub: Hub, cx: &mut TestAppContext) -> (client_core::Client, Heard) {
    // The hub answers on a thread of its own.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let hearing = requests.clone();
    let client = client_core::stand_in_hub::connect(&dir.path().join("hub.sock"), move |request| {
        let sent = if let ClientRequest::ProposalAnswer(answer) = request {
            ProposalRequest::Answer(answer)
        } else if let ClientRequest::ProposalAnotherRound(round) = request {
            ProposalRequest::AnotherRound(round)
        } else {
            return Does::Nothing;
        };
        hearing.lock().push(sent);
        match hub {
            Hub::Refuses => {
                let refused = ApiError::new(ErrorCode::Internal, "the hub went away");
                Does::Answers(Box::new(ReplyOutcome::Error(refused)))
            }
            Hub::Waits => Does::Nothing,
            Hub::Keeps => {
                let kept = ClientResponse::Proposal(protocol::fixtures::proposal("p"));
                Does::Answers(Box::new(ReplyOutcome::Response(kept)))
            }
        }
    });
    (client, Heard { requests, _dir: Arc::new(dir) })
}

/// Whether the view has sent anything, which it says the moment it does.
fn sent_anything(view: &Entity<ProposalView>, cx: &mut TestAppContext) -> bool {
    view.read_with(cx, |view, _| !matches!(view.answering.sending, Sending::Idle))
}

/// The view on a hub that refuses what it sends.
fn view(cx: &mut TestAppContext) -> (Entity<ProposalView>, Heard) {
    view_on(Hub::Refuses, cx)
}

/// Waits until the hub has heard `count` requests, and the view knows how the last went, if the hub answered.
fn until_heard(count: usize, view: &Entity<ProposalView>, heard: &Heard, cx: &mut TestAppContext) {
    waiting::thread::until(Duration::from_secs(10), format!("the hub to hear {count}"), || {
        cx.run_until_parked();
        heard.len() >= count
    });
    waiting::thread::until(Duration::from_secs(10), "the view to hear how it went", || {
        cx.run_until_parked();
        view.read_with(cx, |view, _| !matches!(view.answering.sending, Sending::Going))
    });
}

/// The answers among what the view asked to send.
fn answers(asked: &Heard) -> Vec<ProposalAnswer> {
    let answer = |request: &ProposalRequest| match request {
        ProposalRequest::Answer(answer) => Some(answer.clone()),
        ProposalRequest::AnotherRound(_) => None,
    };
    asked.requests.lock().iter().filter_map(answer).collect()
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

/// After a reconnect, the answer goes on the connection the view was given last, since the one before may be gone, as
/// a phone's is once it has slept.
#[gpui_kit::test]
fn an_answer_goes_on_the_connection_given_last(cx: &mut TestAppContext) {
    let (view, first) = view(cx);
    let (client, last) = stand_in(Hub::Refuses, cx);

    view.update(cx, |view, cx| {
        view.reconnect(&client);
        view.choose(0, Some(OptionPlace(1)), cx);
        view.send(cx);
    });
    until_heard(1, &view, &last, cx);

    assert_eq!(answers(&last).len(), 1);
    assert_eq!(first.len(), 0, "nothing goes to the hub it was given first");
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
