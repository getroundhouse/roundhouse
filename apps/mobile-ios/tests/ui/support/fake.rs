//! A fake hub, which serves a fixed world over the real protocol on a socket, for a test of what the phone does with
//! what a hub holds or how it answers, with no hub, worker or agent to wait for.

use std::sync::Arc;

use client_core::fake_hub::{self, World};
use client_core::{Client, RemoteHub};
use gpui_kit::TestAppContext;
use mobile::{MemoryVault, Vault as _};
use protocol::pairing::Fingerprint;
use tempfile::TempDir;

use super::Phone;

/// A directory of its own, with a fake hub listening there, which goes with it.
pub(crate) struct FakeHub {
    _dir: TempDir,
}

impl<'a> Phone<'a> {
    /// The app on a phone paired with a fake hub holding `world`, which it reaches on the hub's socket. The hub goes
    /// with the test, so it's returned to be kept.
    pub(crate) fn on_a_fake_hub(cx: &'a mut TestAppContext, world: World) -> (FakeHub, Phone<'a>) {
        // Under /tmp because of the socket path limit.
        let dir = tempfile::Builder::new().prefix("phone-fake-").tempdir_in("/tmp").unwrap();
        let socket = dir.path().join("hub.sock");
        fake_hub::serve(&socket, world);
        let vault = Arc::new(MemoryVault::default());
        vault.save(&paired()).unwrap();
        let connect = move |_: &RemoteHub| Client::connect(&socket, "roundhouse-ios");
        let phone = Phone::opening(cx, vault, |vault, opened, options, cx| {
            mobile::open_window_connecting(vault, opened, connect, options, cx)
        });
        (FakeHub { _dir: dir }, phone)
    }
}

/// What the phone keeps of the fake hub it paired with, which its connecting doesn't read.
fn paired() -> RemoteHub {
    RemoteHub {
        hub_name: "Studio".to_string(),
        hosts: vec!["127.0.0.1:1".to_string()],
        fingerprint: Fingerprint::of_sha256([1; 32]),
        device_id: "dev-1".into(),
        token: "made-up".into(),
    }
}
