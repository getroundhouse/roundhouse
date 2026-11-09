//! Reaching a hub over the network, as a device paired with it, such as a phone. It pairs once with a code from a
//! pairing link, keeps the [`RemoteHub`] that gives it, and connects with that from then on.

mod pin;
mod race;
mod tls;
#[cfg(feature = "testing")]
mod write_only;

use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::DeviceId;
use protocol::pairing::{DeviceToken, Fingerprint, PairingLink};
use serde::{Deserialize, Serialize};

use super::transport::Transport;
use super::{Client, ClientError, Result, unexpected};
use race::Lost;
#[cfg(feature = "testing")]
pub use write_only::WriteOnly;

/// What a device needs to connect to a hub it paired with. An app keeps it, as JSON with a string for each field,
/// and `Debug` leaves the token out.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteHub {
    pub hub_name: String,
    /// Each `host:port`, in the order to try them.
    pub hosts: Vec<String>,
    /// The hub's pinned key.
    pub fingerprint: Fingerprint,
    pub device_id: DeviceId,
    pub token: DeviceToken,
}

/// Connects, pairs, and returns what's needed to connect again. A code works once, so if this fails after
/// reaching the hub, pairing again needs a new link.
pub fn pair(link: &PairingLink, device_name: &str) -> Result<RemoteHub> {
    let client = open(&link.hosts, &link.fingerprint)?;
    let reply = client.call(ClientRequest::Pair { code: link.code.clone(), device_name: device_name.to_string() })?;
    let ClientResponse::Paired { device_id, token } = reply else {
        return unexpected(&reply);
    };
    Ok(RemoteHub {
        hub_name: link.hub_name.clone(),
        hosts: link.hosts.clone(),
        fingerprint: link.fingerprint.clone(),
        device_id,
        token,
    })
}

impl Client {
    /// Connects to a hub over the network, as the device that paired with it, and checks the protocol version.
    /// Tries every host at once, favouring them in order. Once connected, it's like any other `Client`, except that
    /// a hub refuses what only its own machine may ask, such as stopping it.
    pub fn connect_remote(hub: &RemoteHub, client_name: &str) -> Result<Client> {
        let client = open(&hub.hosts, &hub.fingerprint)?;
        let reply = client.call(ClientRequest::Authenticate { token: hub.token.clone() })?;
        let ClientResponse::Ok = reply else {
            return unexpected(&reply);
        };
        client.hello(client_name)?;
        Ok(client)
    }

    /// A connection over the network that hasn't authenticated, so a test can see what a hub does with one.
    #[cfg(feature = "testing")]
    pub fn open_remote(hosts: &[String], fingerprint: &Fingerprint) -> Result<Client> {
        open(hosts, fingerprint)
    }
}

fn open(hosts: &[String], fingerprint: &Fingerprint) -> Result<Client> {
    Client::start(reach(hosts, fingerprint)?)
}

/// A connection to whichever of these hosts first answers with the pinned key. If none does, it says the hub has
/// another key only if every host answered with one, since that means pairing again. If any host didn't answer, the
/// hub couldn't be reached, as when its machine is asleep and another machine has an address it had.
fn reach(hosts: &[String], fingerprint: &Fingerprint) -> Result<Transport> {
    race::race(hosts, fingerprint).map_err(|lost| match lost {
        Lost { all_not_pinned: true, not_pinned, .. } => {
            ClientError::FingerprintMismatch { host: not_pinned.into_iter().next().unwrap_or_default() }
        }
        Lost { not_pinned, last, .. } => ClientError::Unreachable { hosts: hosts.to_vec(), last, not_pinned },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app keeps it as JSON, so its form can't change.
    #[test]
    fn a_kept_hub_is_plain_text_and_its_token_stays_out_of_debug_output() {
        let fingerprint = Fingerprint::of_sha256([0; 32]);
        let hub = RemoteHub {
            hub_name: "Mac".to_string(),
            hosts: vec!["mac.local:7867".to_string()],
            fingerprint: fingerprint.clone(),
            device_id: DeviceId::from("d"),
            token: DeviceToken::from("s3cret"),
        };

        let json = serde_json::to_value(&hub).unwrap();

        let expected = serde_json::json!({
            "hub_name": "Mac",
            "hosts": ["mac.local:7867"],
            "fingerprint": fingerprint.as_str(),
            "device_id": "d",
            "token": "s3cret",
        });
        assert_eq!(json, expected);
        let kept: RemoteHub = serde_json::from_value(json).unwrap();
        assert_eq!((kept.fingerprint, kept.token), (hub.fingerprint.clone(), hub.token.clone()));
        assert!(!format!("{hub:?}").contains("s3cret"));
    }

    #[test]
    fn a_hub_that_answers_nowhere_is_unreachable() {
        // The system never hands out port 1 for port 0, which is what tests listen on, so unlike a port another test
        // freed, nothing can take it while this looks.
        let closed = "127.0.0.1:1".to_string();

        let error = open(std::slice::from_ref(&closed), &Fingerprint::of_sha256([0; 32])).map(drop).unwrap_err();

        assert!(matches!(error, ClientError::Unreachable { hosts, .. } if hosts == [closed]));
    }
}
