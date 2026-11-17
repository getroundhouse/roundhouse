//! Starting and cancelling a sign-in against a stand-in hub, which answers each start as a test says and keeps each
//! sign-in it was asked to cancel, so a test can see what the page asked, and when.

use std::sync::mpsc;
use std::time::Duration;

use client_core::ClientError;
use client_core::stand_in_hub::Does;
use gpui_kit::TestAppContext;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::fixtures::github_sign_in;
use protocol::model::{GithubSignIn, GithubSignInId, GithubSignInStatus};

use super::super::kept::Kept;
use super::super::sign_in_words;
use super::stand_in::{WithStandIn, until};

/// How the stand-in answers a sign-in's start: with its record, waiting, or refused as too many waiting.
#[derive(Clone, Copy)]
enum Starts {
    Waiting,
    Refused,
}

/// The page, drawn with a stand-in hub.
struct Page {
    drawn: WithStandIn,
    /// Each sign-in the stand-in was asked to cancel, as it's asked.
    cancels: mpsc::Receiver<GithubSignInId>,
}

fn answer(sign_in: GithubSignIn) -> Does {
    Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::GithubSignIn(sign_in))))
}

fn too_many() -> ApiError {
    ApiError::new(ErrorCode::Conflict, "four are waiting")
}

impl Page {
    /// The page, with a stand-in hub that answers each start as `starts` says.
    fn with_a_hub(cx: &mut TestAppContext, starts: Starts) -> Page {
        let (cancelled, cancels) = mpsc::channel();
        let drawn = WithStandIn::new(cx, move |request| match (request, starts) {
            (ClientRequest::GithubSignInStart(start), Starts::Waiting) => answer(github_sign_in(start.id.as_str())),
            (ClientRequest::GithubSignInStart(_), Starts::Refused) => {
                Does::Answers(Box::new(ReplyOutcome::Error(too_many())))
            }
            (ClientRequest::GithubSignInCancel(cancel), _) => {
                cancelled.send(cancel.id.clone()).unwrap();
                answer(GithubSignIn { status: GithubSignInStatus::Cancelled, ..github_sign_in(cancel.id.as_str()) })
            }
            _ => Does::Nothing,
        });
        Page { drawn, cancels }
    }

    /// Starts a sign-in, and answers the ID the page chose for it.
    fn start(&self, cx: &mut TestAppContext) -> GithubSignInId {
        self.drawn.kept.update(cx, |kept, cx| kept.sign_in(None, cx));
        self.drawn.kept.read_with(cx, |kept, _| kept.sign_in_panel().unwrap().id.clone())
    }

    /// Cancel, in the sign-in's panel.
    fn cancel(&self, cx: &mut TestAppContext) {
        self.drawn.act(cx, Kept::cancel_sign_in);
    }

    /// Whether the panel waits for the hub to answer the start of the sign-in it shows.
    fn starting(&self, cx: &mut TestAppContext) -> bool {
        self.drawn.kept.read_with(cx, |kept, _| kept.sign_in_panel().unwrap().starting)
    }
}

#[gpui_kit::test]
fn a_sign_in_cancelled_before_the_hub_answers_its_start_is_cancelled_once_it_has(cx: &mut TestAppContext) {
    let page = Page::with_a_hub(cx, Starts::Waiting);
    let started = page.start(cx);

    page.cancel(cx);

    page.drawn.kept.read_with(cx, |kept, _| assert!(kept.panel.is_none() && kept.put_away.contains(&started)));
    until(cx, "the cancel", |_| page.cancels.try_recv().is_ok_and(|cancelled| cancelled == started));
}

#[gpui_kit::test]
fn a_waiting_sign_in_is_cancelled_at_once_and_its_panel_put_away(cx: &mut TestAppContext) {
    let page = Page::with_a_hub(cx, Starts::Waiting);
    let started = page.start(cx);
    until(cx, "the hub to start it", |cx| !page.starting(cx));

    page.cancel(cx);

    assert_eq!(page.cancels.recv_timeout(Duration::from_secs(20)).unwrap(), started, "sent as it's cancelled");
    page.drawn.kept.read_with(cx, |kept, _| assert!(kept.panel.is_none()));
}

#[gpui_kit::test]
fn a_start_the_hub_refuses_says_why_in_its_panel_and_cancels_nothing(cx: &mut TestAppContext) {
    let page = Page::with_a_hub(cx, Starts::Refused);
    page.start(cx);

    until(cx, "the refusal", |cx| !page.starting(cx));

    let said = page.drawn.kept.read_with(cx, |kept, _| kept.sign_in_panel().unwrap().not_started.clone());
    assert_eq!(said, Some(sign_in_words::not_started(&ClientError::Api(too_many()))));
    assert!(page.cancels.try_recv().is_err());
}
