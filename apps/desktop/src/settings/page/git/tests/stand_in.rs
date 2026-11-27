//! The page's state, drawn with a stand-in hub, which does with each request what a test says, so the test can see
//! what the page asked, and when.

use client_core::stand_in_hub::{self, Does};
use client_core::{Commands, Hub, RecordsFeed};
use gpui_kit::{AppContext as _, Context, Entity, TestAppContext, Window, WindowHandle};
use protocol::client::ClientRequest;

use super::super::super::stand_in_hub::{Blank, SOON};
use super::super::Asking;
use super::super::kept::Kept;
use super::made_up::records;

/// The page's state, in a window of its own, asking a stand-in hub.
pub(super) struct WithStandIn {
    pub(super) kept: Entity<Kept>,
    pub(super) window: WindowHandle<Blank>,
    /// The calls the page starts on its connection to the stand-in.
    commands: Commands,
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
        let hub = Hub::caught_up_on(&client);
        let asking = Asking { feed: RecordsFeed::start(&client), hub: hub.clone() };
        let commands = hub.commands();
        let window = cx.add_window(|_, _| Blank);
        let kept = cx.new(|_| Kept::default());
        let nothing = records(Vec::new(), Vec::new(), None);
        window
            .update(cx, |_, _, cx| {
                let settings = cx.focus_handle();
                kept.update(cx, |kept, cx| kept.drawn(Some(asking), settings, &nothing, None, cx));
            })
            .unwrap();
        WithStandIn { kept, window, commands, _socket: socket }
    }

    /// Does `act` to the page's state, in its window.
    pub(super) fn act(&self, cx: &mut TestAppContext, act: impl FnOnce(&mut Kept, &mut Window, &mut Context<Kept>)) {
        let kept = self.kept.clone();
        self.window.update(cx, |_, window, cx| kept.update(cx, |kept, cx| act(kept, window, cx))).unwrap();
    }

    /// Once every call the page has started has its answer, whether or not the page has taken it in, which it does
    /// only as the test runs what it waits on. So a call the stand-in hasn't been asked by then was never started.
    pub(super) fn answered(&self) {
        self.commands.finished().recv_timeout(SOON).expect("the stand-in to answer every call the page started");
    }
}
