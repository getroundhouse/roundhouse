//! Starting and cancelling a sign-in against a stand-in hub, which answers each start as a test says and keeps each
//! sign-in it was asked to cancel, so a test can see what the page asked, and when.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use client_core::stand_in_hub::{self, Does};
use client_core::{ClientError, RecordsFeed};
use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, TestAppContext, Window, WindowHandle};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::fixtures::github_sign_in;
use protocol::model::{GithubSignIn, GithubSignInId, GithubSignInStatus};

use super::super::Asking;
use super::super::kept::Kept;
use super::super::sign_in_words;
use super::made_up::records;

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// How the stand-in answers a sign-in's start: with its record, waiting, or refused as too many waiting.
#[derive(Clone, Copy)]
enum Starts {
    Waiting,
    Refused,
}

/// The page, drawn with a stand-in hub.
struct Page {
    kept: Entity<Kept>,
    window: WindowHandle<Blank>,
    /// Each sign-in the stand-in was asked to cancel, as it's asked.
    cancels: mpsc::Receiver<GithubSignInId>,
    /// Where the stand-in listens, which goes with the test.
    _socket: tempfile::TempDir,
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
        // The hub answers on a real thread.
        cx.executor().allow_parking();
        let socket = tempfile::tempdir().unwrap();
        let (cancelled, cancels) = mpsc::channel();
        let client = stand_in_hub::connect(&socket.path().join("hub.sock"), move |request| match (request, starts) {
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
        let asking = Asking { feed: RecordsFeed::start(&client), client };
        let window = cx.add_window(|_, _| Blank);
        let kept = cx.new(|_| Kept::default());
        let nothing = records(Vec::new(), Vec::new(), None);
        window
            .update(cx, |_, _, cx| {
                let settings = cx.focus_handle();
                kept.update(cx, |kept, cx| kept.drawn(Some(asking), settings, &nothing, None, cx));
            })
            .unwrap();
        Page { kept, window, cancels, _socket: socket }
    }

    /// Starts a sign-in, and answers the ID the page chose for it.
    fn start(&self, cx: &mut TestAppContext) -> GithubSignInId {
        self.kept.update(cx, |kept, cx| kept.sign_in(None, cx));
        self.kept.read_with(cx, |kept, _| kept.sign_in_panel().unwrap().id.clone())
    }

    /// Cancel, in the sign-in's panel.
    fn cancel(&self, cx: &mut TestAppContext) {
        let kept = self.kept.clone();
        self.window.update(cx, |_, window, cx| kept.update(cx, |kept, cx| kept.cancel_sign_in(window, cx))).unwrap();
    }

    /// Whether the panel waits for the hub to answer the start of the sign-in it shows.
    fn starting(&self, cx: &mut TestAppContext) -> bool {
        self.kept.read_with(cx, |kept, _| kept.sign_in_panel().unwrap().starting)
    }
}

/// Runs what the page waits on until `done`, or fails after 20 s.
fn until(cx: &mut TestAppContext, what: &str, mut done: impl FnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(cx) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui_kit::test]
fn a_sign_in_cancelled_before_the_hub_answers_its_start_is_cancelled_once_it_has(cx: &mut TestAppContext) {
    let page = Page::with_a_hub(cx, Starts::Waiting);
    let started = page.start(cx);

    page.cancel(cx);

    page.kept.read_with(cx, |kept, _| assert!(kept.panel.is_none() && kept.put_away.contains(&started)));
    until(cx, "the cancel", |_| page.cancels.try_recv().is_ok_and(|cancelled| cancelled == started));
}

#[gpui_kit::test]
fn a_waiting_sign_in_is_cancelled_at_once_and_its_panel_put_away(cx: &mut TestAppContext) {
    let page = Page::with_a_hub(cx, Starts::Waiting);
    let started = page.start(cx);
    until(cx, "the hub to start it", |cx| !page.starting(cx));

    page.cancel(cx);

    assert_eq!(page.cancels.recv_timeout(Duration::from_secs(20)).unwrap(), started, "sent as it's cancelled");
    page.kept.read_with(cx, |kept, _| assert!(kept.panel.is_none()));
}

#[gpui_kit::test]
fn a_start_the_hub_refuses_says_why_in_its_panel_and_cancels_nothing(cx: &mut TestAppContext) {
    let page = Page::with_a_hub(cx, Starts::Refused);
    page.start(cx);

    until(cx, "the refusal", |cx| !page.starting(cx));

    let said = page.kept.read_with(cx, |kept, _| kept.sign_in_panel().unwrap().not_started.clone());
    assert_eq!(said, Some(sign_in_words::not_started(&ClientError::Api(too_many()))));
    assert!(page.cancels.try_recv().is_err());
}
