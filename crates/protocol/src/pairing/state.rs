//! Reaching a hub over the network: the devices paired with it, the secrets they prove themselves with, and the
//! link a phone opens to pair.
//!
//! A hub on a Mac serves its own user through a Unix socket, which is what authenticates them. A device on the
//! network pairs once with a short-lived code it gets from a pairing link, and gets a token it keeps. Each
//! connection after that authenticates with the token before its hello.

use serde::{Deserialize, Serialize};
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory as _};

use crate::client::{ApiError, ErrorCode};
use crate::model::DeviceId;

use super::PairingLink;

/// Enough for any name a phone or computer gives itself.
pub const MAX_DEVICE_NAME_CHARS: usize = 100;

/// Validates the name a device gave itself, trimmed. One with a control or format character is refused, because names
/// are shown in a terminal and in lists, where an escape sequence or a bidi override could change what's shown, and a
/// character with no width could make one name look like another.
pub fn device_name_from(name: &str) -> Result<&str, ApiError> {
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

/// Cleans a host's reported name so [`device_name_from`] accepts it, or returns `fallback` when nothing printable
/// remains.
pub fn clean_device_name(raw: &str, fallback: &str) -> String {
    let filtered: String = raw
        .chars()
        .filter(|&c| !c.is_control() && c.general_category() != GeneralCategory::Format)
        .take(MAX_DEVICE_NAME_CHARS)
        .collect();
    let trimmed = filtered.trim();
    if trimmed.is_empty() { fallback.to_string() } else { trimmed.to_string() }
}

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

    #[expect(unreachable_pub, reason = "a secret's type is public, as the protocol's are, and this is a test's")]
    mod made_up {
        use super::*;

        secrets!(
            /// A secret made up for the test.
            MadeUpSecret,
        );
    }
    use made_up::MadeUpSecret;

    #[test]
    fn a_secret_stays_out_of_debug_output_and_reads_as_plain_text() {
        let secret = MadeUpSecret::from("s3cret");

        assert_eq!(format!("{secret:?}"), "MadeUpSecret(…)");
        assert_eq!(serde_json::to_string(&secret).unwrap(), "\"s3cret\"");
        assert_eq!(serde_json::from_str::<MadeUpSecret>("\"s3cret\"").unwrap(), secret);
        assert_eq!(secret.as_str(), "s3cret");
    }

    #[test]
    fn a_pairing_carries_its_link_as_the_text_a_qr_code_holds() {
        let link = PairingLink::parse(&format!("roundhouse://pair?v=1&name=hub&fp={}&code=c&host=h:1", "f".repeat(43)));
        let pairing = Pairing { link: link.unwrap(), code: PairingCode::from("c"), expires_at_ms: 1 };

        let json = serde_json::to_value(&pairing).unwrap();

        assert_eq!(json["link"], pairing.link.to_string());
        assert_eq!(serde_json::from_value::<Pairing>(json).unwrap(), pairing);
    }
}
