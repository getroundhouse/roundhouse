//! A paired hub that can't be used, which the gallery's app has none of for real.

#![cfg(test)]

use client_core::{OlderSide, RemoteHub};
use desktop::app::ConnectionFailure;
use protocol::pairing::Fingerprint;

use super::Driver;

impl Driver {
    /// Shows a paired hub as across a protocol break, with `older` the older side.
    pub(super) fn across_a_break(&mut self, older: OlderSide) {
        let hub = RemoteHub {
            hub_name: "Studio".to_string(),
            hosts: vec!["studio.local:7867".to_string()],
            fingerprint: Fingerprint::of_sha256([1; 32]),
            device_id: "dev-1".into(),
            token: "made-up".into(),
        };
        let view = self.view.clone();
        let failure = ConnectionFailure::RemoteProtocolBreak(older);
        self.cx.update(|cx| view.update(cx, |view, cx| view.show_failed(hub, failure, cx)));
    }
}
