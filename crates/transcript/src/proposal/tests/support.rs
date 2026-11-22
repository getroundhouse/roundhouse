//! A hub stood in for, which answers what the view sends as a test says and keeps what it heard, and the proposals
//! the tests give the view.

use std::sync::Arc;
use std::time::Duration;

use client_core::stand_in_hub::Does;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use parking_lot::Mutex;
use protocol::client::proposals::ProposalAnswer;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::model::{Choice, OptionPlace, Proposal, ProposalRevision, Response};

use super::super::ProposalView;
use super::super::answering::{ProposalRequest, Sending};

/// What the hub heard the view send, as it heard it, and the folder its socket is in, which goes once the test lets go
/// of this.
#[derive(Clone)]
pub(super) struct Heard {
    requests: Arc<Mutex<Vec<ProposalRequest>>>,
    _dir: Arc<tempfile::TempDir>,
}

impl Heard {
    pub(super) fn len(&self) -> usize {
        self.requests.lock().len()
    }
}

/// How the hub stood in for answers what the view sends.
#[derive(Clone, Copy)]
pub(super) enum Hub {
    /// That it couldn't keep it, as a hub that went away mid-request would.
    Refuses,
    /// Nothing, so what was sent stays on its way.
    Waits,
    /// That it kept it.
    Keeps,
}

/// The view, on a hub that answers what it sends as `hub` says, and everything that hub hears the view send.
pub(super) fn view_on(hub: Hub, cx: &mut TestAppContext) -> (Entity<ProposalView>, Heard) {
    let (client, heard) = stand_in(hub, cx);
    let proposal = Arc::new(protocol::fixtures::proposal("p"));
    let view = cx.update(|cx| cx.new(|cx| ProposalView::new(proposal, client, cx)));
    (view, heard)
}

/// A connection to a hub that answers what the view sends as `hub` says, and everything that hub hears.
pub(super) fn stand_in(hub: Hub, cx: &mut TestAppContext) -> (client_core::Client, Heard) {
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
pub(super) fn sent_anything(view: &Entity<ProposalView>, cx: &mut TestAppContext) -> bool {
    view.read_with(cx, |view, _| !matches!(view.answering.sending, Sending::Idle))
}

/// The view on a hub that refuses what it sends.
pub(super) fn view(cx: &mut TestAppContext) -> (Entity<ProposalView>, Heard) {
    view_on(Hub::Refuses, cx)
}

/// Waits until the hub has heard `count` requests, and the view knows how the last went, if the hub answered.
pub(super) fn until_heard(count: usize, view: &Entity<ProposalView>, heard: &Heard, cx: &mut TestAppContext) {
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
pub(super) fn answers(asked: &Heard) -> Vec<ProposalAnswer> {
    let answer = |request: &ProposalRequest| match request {
        ProposalRequest::Answer(answer) => Some(answer.clone()),
        ProposalRequest::AnotherRound(_) => None,
    };
    asked.requests.lock().iter().filter_map(answer).collect()
}

pub(super) fn responded(response: Response) -> Arc<Proposal> {
    let proposal = protocol::fixtures::proposal("p");
    Arc::new(Proposal { current: ProposalRevision { response: Some(response), ..proposal.current }, ..proposal })
}

pub(super) fn answered() -> Arc<Proposal> {
    responded(Response::Chose(Choice { chosen: vec![OptionPlace(1)], note: String::new(), at_ms: 0 }))
}
