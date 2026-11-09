//! Reaching a hub over the network, as a device paired with it, such as a phone. It pairs once with a code from a
//! pairing link, keeps the [`RemoteHub`] that gives it, and connects with that from then on.

mod pin;
mod race;
mod tls;

use std::fmt;
use std::time::Duration;

use protocol::client::{ClientRequest, ClientResponse};
use protocol::pairing::{DeviceToken, PairingCode, PairingLink};
use serde::{Deserialize, Serialize};

use super::{Client, ClientError, Result, unexpected};
use race::Lost;

/// How long to wait for each lookup, and each address to connect: long enough for a hub on the same network or
/// tailnet, and short enough that a device away from both gives up quickly.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// What a device needs to connect to a hub it paired with. An app keeps it, and the token in it is a secret, so
/// `Debug` leaves the token out.
#[derive(Clone, Serialize, Deserialize)]
pub struct RemoteHub {
    pub hub_name: String,
    /// Each `host:port`, in the order to try them.
    pub hosts: Vec<String>,
    /// The hub's pinned key. See [`PairingLink::fingerprint`].
    pub fingerprint: String,
    pub device_id: String,
    pub token: String,
}

impl fmt::Debug for RemoteHub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteHub")
            .field("hub_name", &self.hub_name)
            .field("hosts", &self.hosts)
            .field("fingerprint", &self.fingerprint)
            .field("device_id", &self.device_id)
            .field("token", &"…")
            .finish()
    }
}

/// Connects, pairs, and returns what's needed to connect again. A code works once, so if this fails after
/// reaching the hub, pairing again needs a new link.
pub fn pair(link: &PairingLink, device_name: &str) -> Result<RemoteHub> {
    let client = open(&link.hosts, &link.fingerprint)?;
    let request =
        ClientRequest::Pair { code: PairingCode::from(link.code.as_str()), device_name: device_name.to_string() };
    let reply = client.call(request)?;
    let ClientResponse::Paired { device_id, token } = reply else {
        return unexpected(&reply);
    };
    Ok(RemoteHub {
        hub_name: link.hub_name.clone(),
        hosts: link.hosts.clone(),
        fingerprint: link.fingerprint.clone(),
        device_id: device_id.to_string(),
        token: token.as_str().to_string(),
    })
}

impl Client {
    /// Connects to a hub over the network, as the device that paired with it, and checks the protocol version.
    /// Tries every host at once, favouring them in order. Once connected, it's like any other `Client`, except that a hub refuses what only
    /// its own machine may ask, such as stopping it.
    pub fn connect_remote(hub: &RemoteHub, client_name: &str) -> Result<Client> {
        let client = open(&hub.hosts, &hub.fingerprint)?;
        let reply = client.call(ClientRequest::Authenticate { token: DeviceToken::from(hub.token.as_str()) })?;
        let ClientResponse::Ok = reply else {
            return unexpected(&reply);
        };
        client.hello(client_name)?;
        Ok(client)
    }

    /// A connection over the network that hasn't authenticated, so a test can see what a hub does with one.
    #[cfg(feature = "testing")]
    pub fn open_remote(hosts: &[String], fingerprint: &str) -> Result<Client> {
        open(hosts, fingerprint)
    }
}

/// A connection to whichever of these hosts first answers with the pinned key. If none does, says so, and that one
/// answered with another key, if any did, since that needs pairing again rather than waiting.
fn open(hosts: &[String], fingerprint: &str) -> Result<Client> {
    match race::race(hosts, fingerprint, CONNECT_TIMEOUT) {
        Ok(transport) => Client::start(transport),
        Err(Lost { not_pinned: Some(host), .. }) => Err(ClientError::FingerprintMismatch { host }),
        Err(Lost { not_pinned: None, last }) => Err(ClientError::Unreachable { hosts: hosts.to_vec(), last }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_hub_reads_back_and_its_token_stays_out_of_debug_output() {
        let hub = RemoteHub {
            hub_name: "Mac".to_string(),
            hosts: vec!["mac.local:7867".to_string()],
            fingerprint: "f".to_string(),
            device_id: "d".to_string(),
            token: "s3cret".to_string(),
        };

        let kept: RemoteHub = serde_json::from_str(&serde_json::to_string(&hub).unwrap()).unwrap();

        assert_eq!((kept.hosts, kept.token), (hub.hosts.clone(), hub.token.clone()));
        assert!(!format!("{hub:?}").contains("s3cret"));
    }

    #[test]
    fn a_hub_that_answers_nowhere_is_unreachable() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string();

        let error = open(std::slice::from_ref(&closed), "f").map(drop).unwrap_err();

        assert!(matches!(error, ClientError::Unreachable { hosts, .. } if hosts == [closed]));
    }
}
