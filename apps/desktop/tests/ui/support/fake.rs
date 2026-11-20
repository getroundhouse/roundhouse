//! A fake hub, which serves a fixed [`World`] over the real protocol, for a test of what the window draws of what a
//! hub holds, with no hub, worker or agent to wait for.

#![cfg(test)]

use std::path::{Path, PathBuf};

use client_core::fake_hub::{self, World};
use desktop::app::HubSource;
use desktop::checkout::Checkout;
use gpui_kit::TestAppContext;
use local_hub::Daemon;
use protocol::event::EventBody;
use protocol::model::{SessionId, SessionStatus};
use tempfile::TempDir;

use super::Ui;

/// A directory of its own, with a fake hub listening there. The hub's thread listens until the process ends, since
/// nothing stops it, which costs a thread a test, and its socket goes with the directory.
pub(crate) struct FakeHub {
    _dir: TempDir,
    hub: HubSource,
}

impl FakeHub {
    pub(crate) fn holding(world: World) -> Self {
        // Under /tmp because of the socket path limit.
        let dir = tempfile::Builder::new().prefix("fake-").tempdir_in("/tmp").unwrap();
        let hub = hub_source(dir.path());
        std::fs::create_dir_all(&hub.paths.data_dir).unwrap();
        fake_hub::serve(&hub.paths.socket, world);
        Self { _dir: dir, hub }
    }

    /// Where the app keeps its own settings while it's open on this hub, as [`super::Stack::settings_file`] says for
    /// a real one.
    pub(crate) fn settings_file(&self) -> PathBuf {
        self.hub.paths.data_dir.join("settings.json")
    }

    /// The hub as the app is told of it, for a test that opens another window on it.
    pub(crate) fn source(&self) -> &HubSource {
        &self.hub
    }
}

/// Somewhere of its own, with a daemon that doesn't exist, so the app can't start a real hub in its place. The
/// daemon's path is the same every run, because the app shows it when it can't start one.
pub(crate) fn hub_source(dir: &Path) -> HubSource {
    let paths = protocol::paths::resolve(|name| match name {
        protocol::paths::ENV_HOME => Some(dir.join("home").to_string_lossy().into_owned()),
        protocol::paths::ENV_WORKSPACES => Some(dir.join("workspaces").to_string_lossy().into_owned()),
        _ => None,
    })
    .unwrap();
    let daemon = Daemon::in_place("/nonexistent/roundhoused");
    HubSource {
        remote: None,
        paths,
        daemon,
        build_number: Some(fake_hub::BUILD_NUMBER),
        build_version: fake_hub::BUILD,
    }
}

impl<'a> Ui<'a> {
    /// Opens the window on the fake hub, as a stamped build, so with no checkout, and waits for it to connect.
    pub(crate) fn open_on(cx: &'a mut TestAppContext, fake: &FakeHub) -> Self {
        Self::open_on_from(cx, fake, None)
    }

    /// The same, told it was built from `checkout`, for a test of what the window says of where it was built.
    pub(crate) fn open_on_from(cx: &'a mut TestAppContext, fake: &FakeHub, checkout: Option<Checkout>) -> Self {
        let mut ui = Self::open_window(cx, fake.hub.clone(), checkout);
        ui.wait_to_connect();
        ui
    }
}

/// A world of one workspace, with one chat in it, of `events`, in `status`, and that chat. A test changes the chat's
/// record as the hub would have it through the world's `sessions`, of which it's the last.
pub(crate) fn one_chat(status: SessionStatus, events: Vec<EventBody>) -> (World, SessionId) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let chat = world.chat(&workspace, status, events);
    (world, chat)
}

/// The window on a fake hub holding `world`, showing `chat`. The hub goes with the test, so it's returned to be kept.
pub(crate) fn open_on_the_chat<'a>(cx: &'a mut TestAppContext, world: World, chat: &SessionId) -> (FakeHub, Ui<'a>) {
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    ui.activate_chat(chat);
    (fake, ui)
}
