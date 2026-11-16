//! Connection metadata is local; device tokens belong in the system credential store.

use client_core::RemoteHub;
use gpui_kit::{App, AsyncApp, Window};
use protocol::model::DeviceId;
use protocol::pairing::{DeviceToken, Fingerprint};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

use super::{AppView, hubs};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedHub {
    pub(crate) name: String,
    pub(crate) hosts: Vec<String>,
    pub(crate) fingerprint: Fingerprint,
    pub(crate) device: DeviceId,
}

impl SavedHub {
    pub(crate) fn of(hub: &RemoteHub) -> Self {
        Self {
            name: hub.hub_name.clone(),
            hosts: hub.hosts.clone(),
            fingerprint: hub.fingerprint.clone(),
            device: hub.device_id.clone(),
        }
    }

    fn remote(self, token: String) -> RemoteHub {
        RemoteHub {
            hub_name: self.name,
            hosts: self.hosts,
            fingerprint: self.fingerprint,
            device_id: self.device,
            token: DeviceToken::from(token),
        }
    }
}

/// The data directory scopes credentials, so a development checkout cannot use the installed app's pairing.
pub(crate) fn credential_key(data: &std::path::Path, device: &DeviceId) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, data.to_string_lossy().as_bytes());
    let mut scope = String::with_capacity(64);
    for byte in digest.as_ref() {
        let _ = write!(scope, "{byte:02x}");
    }
    format!("roundhouse/{scope}/{device}")
}

pub(crate) async fn keep(hub: &RemoteHub, key: String, cx: &mut AsyncApp) -> anyhow::Result<()> {
    let token = hub.token.as_str().as_bytes().to_vec();
    let task = cx.update(|cx| cx.write_credentials(&key, &hub.device_id.to_string(), &token));
    task.await?;
    let saved = SavedHub::of(hub);
    cx.update(|cx| {
        crate::settings::change(cx, |settings| {
            settings.hubs.retain(|old| old.fingerprint != saved.fingerprint);
            settings.hubs.push(saved);
        });
    });
    Ok(())
}

pub(crate) fn open(saved: SavedHub, view: gpui_kit::WeakEntity<AppView>, window: &mut Window, cx: &mut App) {
    let intent = hubs::begin_selection(cx);
    let Some(current) = view.upgrade() else { return };
    let key = credential_key(&current.read(cx).hub.paths.data_dir, &saved.device);
    let reading = cx.read_credentials(&key);
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let result = async {
            let (_, token) = reading.await?.ok_or_else(|| {
                anyhow::anyhow!("This hub has no saved credential. Connect it again with a new pairing link.")
            })?;
            Ok::<_, anyhow::Error>(saved.remote(String::from_utf8(token)?))
        }
        .await;
        match result {
            Ok(remote) => {
                let _ = handle.update(cx, |_, window, cx| hubs::finish_selection(intent, remote, window, cx));
            }
            Err(error) => {
                let _ = view.update(cx, |view, cx| view.show_error(error.to_string(), cx));
            }
        }
    })
    .detach();
}
