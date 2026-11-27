//! A sign-in against a stand-in hub, for what a real one's timing can't be had to show: one put away before the hub
//! answers its start, what the person was sent back with lost with the connection, and nobody sent back in time.

#![cfg(all(test, unix))]

use std::sync::Arc;
use std::time::Duration;

use client_core::stand_in_hub::{self, Does};
use client_core::{Client, Hub};
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use parking_lot::Mutex;
use protocol::client::tool_sign_ins::{ToolSignInBegun, ToolSignInPage};
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome};
use protocol::fixtures::tool_server;

use super::{NewServer, SignInTo, Stands, ToolSigningIn};
use crate::loopback::browsing::fetch;
use crate::timers::Timers;

/// How long a test waits for what a stand-in hub answers at once.
const SOON: Duration = Duration::from_secs(5);

/// What a stand-in was asked about a sign-in: what, and where its person is sent back to or what they were sent back
/// with, where that's said.
#[derive(Clone, Default)]
struct Heard(Arc<Mutex<Vec<(&'static str, String)>>>);

impl Heard {
    fn what(&self) -> Vec<&'static str> {
        self.0.lock().iter().map(|(what, _)| *what).collect()
    }

    fn said(&self, what: &str) -> Vec<String> {
        self.0.lock().iter().filter(|(asked, _)| *asked == what).map(|(_, said)| said.clone()).collect()
    }
}

fn says(response: ClientResponse) -> Does {
    Does::Answers(Box::new(ReplyOutcome::Response(response)))
}

fn page() -> Does {
    let page = ToolSignInPage::new("https://auth.example.com/authorize?state=made-up");
    says(ClientResponse::ToolSignInBegun(ToolSignInBegun::Page { page }))
}

/// A stand-in hub, which does with each start and finish what's said, and keeps what it's asked.
fn stand_in(
    dir: &tempfile::TempDir,
    name: &str,
    heard: &Heard,
    starts: impl Fn() -> Does + Send + 'static,
    finishes: impl Fn() -> Does + Send + 'static,
) -> Client {
    let heard = heard.clone();
    stand_in_hub::connect(&dir.path().join(name), move |request| {
        let (asked, does) = match request {
            ClientRequest::ToolSignInStart(start) => (("start", start.redirect), starts()),
            ClientRequest::ToolSignInFinish(finish) => (("finish", finish.returned.as_str().to_string()), finishes()),
            ClientRequest::ToolSignInCancel(_) => (("cancel", String::new()), says(ClientResponse::Ok)),
            _ => return Does::Nothing,
        };
        heard.0.lock().push(asked);
        does
    })
}

fn linear() -> SignInTo {
    let url = "https://mcp.example.com/mcp".to_string();
    SignInTo::New(NewServer { id: "t".into(), name: "Linear".into(), url })
}

fn signing_in(cx: &mut TestAppContext, hub: &Hub) -> Entity<ToolSigningIn> {
    // The hub answers on a real thread.
    cx.executor().allow_parking();
    let signing_in = cx.new(|_| ToolSigningIn::default());
    signing_in.update(cx, |signing_in, cx| signing_in.hub_is(Some(hub.clone()), cx));
    signing_in
}

fn until(cx: &mut TestAppContext, what: &str, done: impl FnMut(&mut TestAppContext) -> bool) {
    crate::executor_waits::until(cx, SOON, format!("timed out waiting for {what}"), done);
}

fn until_waiting(cx: &mut TestAppContext, signing_in: &Entity<ToolSigningIn>) {
    until(cx, "the page", |cx| {
        let stands = |signing_in: &ToolSigningIn| {
            signing_in.under_way().is_some_and(|under_way| matches!(under_way.stands(), Stands::Waiting(_)))
        };
        signing_in.read_with(cx, |signing_in, _| stands(signing_in))
    });
}

#[gpui_kit::test]
fn one_put_away_before_the_hub_answers_its_start_is_cancelled_once_it_has_and_opens_nothing(cx: &mut TestAppContext) {
    let (dir, heard) = (tempfile::tempdir().unwrap(), Heard::default());
    let (answer, answers) = flume::bounded(1);
    let after = move || Does::After(answers.clone(), Box::new(page()));
    let hub = Hub::caught_up_on(&stand_in(&dir, "hub", &heard, after, || Does::Nothing));
    let signing_in = signing_in(cx, &hub);
    signing_in.update(cx, |signing_in, cx| signing_in.start(linear(), cx));
    until(cx, "the start", |_| heard.what() == ["start"]);
    let stands = |signing_in: &ToolSigningIn, _: &gpui_kit::App| {
        matches!(signing_in.under_way().map(super::UnderWay::stands), Some(Stands::Starting(_)))
    };
    assert!(signing_in.read_with(cx, stands), "under way, while the hub finds out how");

    signing_in.update(cx, ToolSigningIn::put_away);

    until(cx, "the cancel", |_| heard.what() == ["start", "cancel"]);
    answer.send(()).unwrap();
    until(cx, "the cancel once there's one to cancel", |_| heard.what() == ["start", "cancel", "cancel"]);
    assert_eq!(cx.opened_url(), None);
    assert!(signing_in.read_with(cx, |signing_in, _| signing_in.under_way().is_none()));
    let redirect = &heard.said("start")[0];
    let at = redirect.strip_prefix("http://").unwrap().strip_suffix("/callback").unwrap();
    // Waited for, since the port is a fixed one, which another test's sign-in can have for a moment.
    // Not by connecting, which one that's bound and doesn't serve refuses too, once enough have.
    until(cx, "nothing to listen there", |_| std::net::TcpListener::bind(at).is_ok());
}

#[gpui_kit::test]
fn what_the_person_was_sent_back_with_is_sent_again_on_the_next_connection_once_one_loses_it(cx: &mut TestAppContext) {
    let (dir, heard) = (tempfile::tempdir().unwrap(), Heard::default());
    let hub = Hub::caught_up_on(&stand_in(&dir, "first", &heard, page, || Does::HangsUp));
    let signing_in = signing_in(cx, &hub);
    signing_in.update(cx, |signing_in, cx| signing_in.start(linear(), cx));
    until_waiting(cx, &signing_in);
    let back = format!("{}?code=made-up&state=made-up", heard.said("start")[0]);

    assert_eq!(fetch(&back).status, 200);

    until(cx, "the finish that's lost", |_| heard.what() == ["start", "finish"]);
    let second = stand_in(&dir, "second", &heard, page, || says(ClientResponse::ToolServer(tool_server("t"))));
    hub.carry_on_to(&second);
    signing_in.update(cx, |signing_in, cx| {
        signing_in.hub_is(None, cx);
        signing_in.hub_is(Some(hub.clone()), cx);
        // As each change of what the window shows of the hub tells it.
        signing_in.hub_is(Some(hub.clone()), cx);
    });
    until(cx, "it to have signed in", |cx| signing_in.read_with(cx, |signing_in, _| signing_in.under_way().is_none()));
    assert_eq!(heard.said("finish"), ["code=made-up&state=made-up", "code=made-up&state=made-up"]);
}

#[gpui_kit::test]
fn one_nobody_approves_in_time_says_so_and_is_cancelled(cx: &mut TestAppContext) {
    let (dir, heard) = (tempfile::tempdir().unwrap(), Heard::default());
    cx.update(|cx| cx.set_global(Timers { tool_sign_in_sends_back: Duration::from_millis(50), ..Timers::DEFAULT }));
    let hub = Hub::caught_up_on(&stand_in(&dir, "hub", &heard, page, || Does::Nothing));
    let signing_in = signing_in(cx, &hub);

    signing_in.update(cx, |signing_in, cx| signing_in.start(linear(), cx));

    until(cx, "the cancel", |_| heard.what() == ["start", "cancel"]);
    let why = signing_in.read_with(cx, |signing_in, _| match signing_in.under_way().map(super::UnderWay::stands) {
        Some(Stands::Failed(why)) => why.clone(),
        _ => String::new(),
    });
    assert_eq!(why, "It wasn't approved within 15 minutes, so the sign-in ended.");
    assert_eq!(cx.opened_url().as_deref(), Some("https://auth.example.com/authorize?state=made-up"));
}
