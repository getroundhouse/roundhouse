//! Pairing devices, authenticating them, and revoking them, and listening on the network only while a device
//! could connect.

use std::sync::Arc;
use std::time::Instant;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::DeviceId;
use protocol::pairing::{Device, DeviceToken, Pairing, PairingCode, PairingLink};
use tokio::sync::MutexGuard;
use tracing::{info, warn};
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

use super::{Hub, new_id};
use crate::error::internal;
use crate::remote::codes::Waiting;
use crate::remote::machine::this_machine;
use crate::remote::{DeviceLink, Revoke, random_text};
use crate::store::{Store, TokenHash, now_ms};

/// Enough for any name a phone gives itself.
const MAX_DEVICE_NAME_CHARS: usize = 100;

fn unauthorized(message: &str) -> ApiError {
    ApiError::new(ErrorCode::Unauthorized, message)
}

/// The name a device gave itself, trimmed. One with a control or format character is refused, because names are
/// shown in a terminal and in lists, where an escape sequence or a bidi override could change what's shown, and a
/// character with no width could make one name look like another.
fn device_name_from(name: &str) -> Result<&str, ApiError> {
    let name = name.trim();
    let invalid = |message: String| Err(ApiError::new(ErrorCode::InvalidRequest, message));
    if name.is_empty() || name.chars().count() > MAX_DEVICE_NAME_CHARS {
        return invalid(format!("a device's name has to be 1 to {MAX_DEVICE_NAME_CHARS} characters"));
    }
    if name.chars().any(|c| c.is_control() || c.general_category() == GeneralCategory::Format) {
        return invalid(format!("a device's name can't have control or format characters, as {name:?} does"));
    }
    Ok(name)
}

impl Hub {
    /// At startup, a hub with paired devices listens for them. One that can't says why in its log, and the socket
    /// still works.
    pub(crate) async fn listen_if_paired(self: &Arc<Self>) {
        let _changing = self.remote.changing.lock().await;
        match self.blocking(Store::has_devices).await {
            Ok(false) => {}
            Ok(true) => {
                if let Err(e) = self.remote.listen(self).await {
                    warn!("not listening for paired devices: {e}");
                }
            }
            Err(e) => warn!("not listening for paired devices, because they couldn't be read: {e}"),
        }
    }

    pub(crate) async fn pairing_start(self: &Arc<Self>) -> Result<Pairing, ApiError> {
        let machine = tokio::task::spawn_blocking(this_machine).await.map_err(|e| internal(e.to_string()))?;
        if machine.hosts.is_empty() {
            return Err(ApiError::new(ErrorCode::Conflict, "this machine has no address a phone could reach"));
        }
        let code = PairingCode::from(random_text::<16>()?);
        let _changing = self.remote.changing.lock().await;
        let port = self.remote.listen(self).await?;
        let fingerprint = self.remote.fingerprint().await?;
        let life = self.remote.pairing_life;
        let expires_at = self.remote.codes.lock().add(code.clone(), Instant::now(), life);
        let hub = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep_until(expires_at.into()).await;
            hub.codes_expired().await;
        });
        let link = PairingLink {
            hub_name: machine.name,
            hosts: machine.hosts.iter().map(|host| format!("{host}:{port}")).collect(),
            fingerprint,
            code: code.clone(),
        };
        let expires_at_ms = now_ms().saturating_add(i64::try_from(life.as_millis()).unwrap_or(i64::MAX));
        Ok(Pairing { link, code, expires_at_ms })
    }

    pub(crate) async fn pairing_wait(&self, code: PairingCode) -> Result<Device, ApiError> {
        loop {
            // Listening before looking, so a device that pairs in between still wakes this.
            let paired = self.remote.paired.notified();
            tokio::pin!(paired);
            paired.as_mut().enable();
            let waiting = self.remote.codes.lock().waiting(&code, Instant::now());
            match waiting {
                Waiting::Paired(device) => return Ok(device),
                Waiting::Gone => return Err(ApiError::new(ErrorCode::NotFound, "that pairing code has expired")),
                Waiting::Until(expires_at) => {
                    let _ = tokio::time::timeout_at(expires_at.into(), paired).await;
                }
            }
        }
    }

    pub(crate) async fn device_list(&self) -> Result<Vec<Device>, ApiError> {
        let mut devices = self.blocking(Store::list_devices).await?;
        for device in &mut devices {
            device.connections = self.remote.connection_count(&device.id);
        }
        Ok(devices)
    }

    pub(crate) async fn device_revoke(&self, id: DeviceId) -> Result<(), ApiError> {
        let changing = self.remote.changing.lock().await;
        let removing = id.clone();
        if self.blocking(move |store| store.remove_device(&removing)).await? {
            info!(device = %id, "revoked a device");
        }
        self.remote.close_all(&id);
        self.stop_listening_if_unneeded(&changing).await
    }

    /// The first request on a connection from the network, with a code from a pairing link. Revoking the device
    /// sets the connection's `revoke`.
    pub(crate) async fn pair(
        &self,
        code: &PairingCode,
        device_name: &str,
        revoke: &Revoke,
    ) -> Result<(DeviceToken, DeviceLink), ApiError> {
        let name = device_name_from(device_name)?;
        let wrong_code =
            || unauthorized("that pairing code is wrong, used or expired. Make a new one on the hub's machine");
        // Checked before waiting on anything, so a connection with a wrong code holds up nobody.
        if !self.remote.codes.lock().is_live(code, Instant::now()) {
            return Err(wrong_code());
        }
        let _changing = self.remote.changing.lock().await;
        if !self.remote.codes.lock().claim(code, Instant::now()) {
            return Err(wrong_code());
        }
        let token = DeviceToken::from(random_text::<32>()?);
        let now = now_ms();
        let device = Device {
            id: new_id().into(),
            name: name.to_string(),
            paired_at_ms: now,
            last_seen_ms: now,
            connections: 0,
        };
        let (saving, hash) = (device.clone(), TokenHash::of(&token));
        self.blocking(move |store| store.add_device(&saving, &hash)).await?;
        info!(device = %device.id, name = device.name, "paired a device");
        let link = self.remote.connected(&device.id, revoke);
        self.remote.codes.lock().paired(code, device);
        self.remote.paired.notify_waiters();
        Ok((token, link))
    }

    /// The first request on a connection from a device that has paired. Revoking the device sets the connection's
    /// `revoke`.
    pub(crate) async fn authenticate(&self, token: &DeviceToken, revoke: &Revoke) -> Result<DeviceLink, ApiError> {
        let not_paired = || unauthorized("this device isn't paired with this hub, or was revoked. Pair it again");
        let hash = TokenHash::of(token);
        // Looked up before waiting on anything, so a connection with a wrong token holds up nobody.
        let device = self.blocking(move |store| store.device_with_token(&hash)).await?.ok_or_else(not_paired)?;
        // A revoke holds this while it removes the device and closes its connections. So the device is either gone
        // by now, and noting it was seen says so, or this connection is noted before the revoke looks for it.
        let _changing = self.remote.changing.lock().await;
        let (id, now) = (device.id.clone(), now_ms());
        if !self.blocking(move |store| store.device_seen(&id, now)).await? {
            return Err(not_paired());
        }
        Ok(self.remote.connected(&device.id, revoke))
    }

    pub(crate) async fn device_disconnected(&self, link: &DeviceLink) {
        self.remote.disconnected(link);
        let (id, now) = (link.device_id.clone(), now_ms());
        if let Err(e) = self.blocking(move |store| store.device_seen(&id, now)).await {
            warn!(device = %link.device_id, "noting when a device was last seen failed: {e}");
        }
    }

    async fn codes_expired(&self) {
        let changing = self.remote.changing.lock().await;
        self.remote.codes.lock().forget_expired(Instant::now());
        if let Err(e) = self.stop_listening_if_unneeded(&changing).await {
            warn!("checking whether to keep listening for devices failed: {e}");
        }
    }

    /// Stops listening once no device is paired and no code is open. Takes the guard to show it's held.
    async fn stop_listening_if_unneeded(&self, _changing: &MutexGuard<'_, ()>) -> Result<(), ApiError> {
        let open = self.remote.codes.lock().any_open(Instant::now());
        if !open && !self.blocking(Store::has_devices).await? {
            self.remote.stop_listening().await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_name_is_trimmed_and_has_no_control_or_format_characters() {
        assert_eq!(device_name_from("  Ada’s iPhone ").unwrap(), "Ada’s iPhone");
        assert_eq!(device_name_from("Ada’s 📱 Pro").unwrap(), "Ada’s 📱 Pro");
        let controls = ["", "   ", "\u{1b}[2J\u{1b}[Hhub", "two\nlines", "bell\u{7}", "csi\u{9b}2J"];
        // A right-to-left override, a zero-width space, joiner and no-break space, and a soft hyphen.
        let formats = ["Ada\u{202e}enohp", "Ad\u{200b}a", "Ad\u{200d}a", "\u{feff}Ada", "A\u{ad}da"];
        for refused in controls.into_iter().chain(formats) {
            let error = device_name_from(refused).map(drop).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidRequest, "{refused:?}");
        }
        assert!(device_name_from(&"x".repeat(MAX_DEVICE_NAME_CHARS + 1)).is_err());
    }
}
