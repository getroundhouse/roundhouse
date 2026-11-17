//! The page's state, drawn with a stand-in hub, which does with each request what a test says, so the test can see
//! what the page asked, and when.

use std::time::{Duration, Instant};

use client_core::RecordsFeed;
use client_core::stand_in_hub::{self, Does};
use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, TestAppContext, Window, WindowHandle};
use protocol::client::ClientRequest;

use super::super::Asking;
use super::super::kept::Kept;
use super::made_up::records;

pub(super) struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// The page's state, in a window of its own, asking a stand-in hub.
pub(super) struct WithStandIn {
    pub(super) kept: Entity<Kept>,
    pub(super) window: WindowHandle<Blank>,
    /// Where the stand-in listens, which goes with the test.
    _socket: tempfile::TempDir,
}

impl WithStandIn {
    /// The page, with a stand-in hub that does with each request what `does` says.
    pub(super) fn new(cx: &mut TestAppContext, does: impl FnMut(ClientRequest) -> Does + Send + 'static) -> Self {
        // The hub answers on a real thread.
        cx.executor().allow_parking();
        let socket = tempfile::tempdir().unwrap();
        let client = stand_in_hub::connect(&socket.path().join("hub.sock"), does);
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
        WithStandIn { kept, window, _socket: socket }
    }

    /// Does `act` to the page's state, in its window.
    pub(super) fn act(&self, cx: &mut TestAppContext, act: impl FnOnce(&mut Kept, &mut Window, &mut Context<Kept>)) {
        let kept = self.kept.clone();
        self.window.update(cx, |_, window, cx| kept.update(cx, |kept, cx| act(kept, window, cx))).unwrap();
    }
}

/// Runs what the page waits on until `done`, or fails after 20 s.
pub(super) fn until(cx: &mut TestAppContext, what: &str, mut done: impl FnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(cx) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
}
