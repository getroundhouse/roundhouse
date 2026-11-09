//! Reaching a hub over the network: the devices paired with it, the secrets they prove themselves with, and the
//! link a phone opens to pair.
//!
//! A hub on a Mac serves its own user through a Unix socket, which is what authenticates them. A device on the
//! network pairs once with a short-lived code it gets from a pairing link, and gets a token it keeps. Each
//! connection after that authenticates with the token before its hello.

use serde::{Deserialize, Serialize};

use crate::model::DeviceId;

mod fingerprint;
#[cfg(test)]
mod laws;
mod link;

pub use fingerprint::{Fingerprint, InvalidFingerprint};
pub use link::{LINK_VERSION, LinkError, PairingLink};

/// Declares secrets: text that's only compared and passed along, which `Debug` leaves out, so a request or
/// reply in a log can't give one away. They're plain strings on the wire.
macro_rules! secrets {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($name), "(…)"))
            }
        }

        impl From<String> for $name {
            fn from(secret: String) -> Self {
                Self(secret)
            }
        }

        impl From<&str> for $name {
            fn from(secret: &str) -> Self {
                Self(secret.to_string())
            }
        }
    )+};
}

secrets!(
    /// Pairs one device, once, for a few minutes after the hub made it. The hub keeps it only in memory.
    PairingCode,
    /// What a paired device proves itself with on each connection. Only the device has it: the hub keeps its
    /// hash.
    DeviceToken,
);

/// A device paired with a hub, such as a phone. Only a client on the hub's own machine can list them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    /// What the device called itself when it paired, such as "Ada's iPhone".
    pub name: String,
    pub paired_at_ms: i64,
    /// When it last authenticated or disconnected.
    pub last_seen_ms: i64,
    /// Connections it has open now.
    #[serde(default)]
    pub connections: u32,
}

/// A new pairing code, and the link that carries it to a phone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Pairing {
    /// Text on the wire, as in a QR code.
    pub link: PairingLink,
    /// The code in the link, for waiting on it.
    pub code: PairingCode,
    pub expires_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_stays_out_of_debug_output_and_reads_as_plain_text() {
        let token = DeviceToken::from("s3cret");

        assert_eq!(format!("{token:?}"), "DeviceToken(…)");
        assert_eq!(serde_json::to_string(&token).unwrap(), "\"s3cret\"");
    }

    #[test]
    fn a_pairing_carries_its_link_as_the_text_a_qr_code_holds() {
        let link = PairingLink::parse(&format!("roundhouse://pair?v=1&name=hub&fp={}&code=c&host=h:1", "f".repeat(43)));
        let pairing = Pairing { link: link.unwrap(), code: PairingCode::from("c"), expires_at_ms: 1 };

        let json = serde_json::to_value(&pairing).unwrap();

        assert_eq!(json["link"], pairing.link.to_string());
        assert_eq!(serde_json::from_value::<Pairing>(json).unwrap(), pairing);
    }

    /// A client that gets a reply it didn't expect puts the reply in its error.
    #[test]
    fn a_pairing_code_stays_out_of_debug_output_wherever_it_is() {
        let link = PairingLink {
            hub_name: "hub".to_string(),
            hosts: vec!["hub.local:7867".to_string()],
            fingerprint: Fingerprint::of_sha256([0; 32]),
            code: PairingCode::from("s3cret"),
        };
        let pairing = Pairing { link: link.clone(), code: link.code.clone(), expires_at_ms: 1 };

        let shown = format!("{link:?} {:?}", crate::client::ClientResponse::Pairing(pairing));

        assert!(!shown.contains("s3cret"), "{shown}");
        assert!(shown.contains("hub.local:7867"), "{shown}");
    }
}
