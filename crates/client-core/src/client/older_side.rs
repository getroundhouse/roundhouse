//! Which side of a protocol break is older, which says what to update to cross it.

use protocol::client::{ApiError, CLIENT_PROTOCOL_VERSION, ErrorCode};

use super::ClientError;

/// Which of this client and a hub is older, when they share no version of the protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OlderSide {
    /// The hub, of the release it said. A hub from before one that refuses a hello said its release says none.
    Hub { version: Option<String> },
    /// This client. The hub is of this release.
    Client { hub_version: String },
}

impl ClientError {
    /// Which side is older, if there's a real break between this client and the hub. A hub that doesn't say what it
    /// speaks is of a build from before hubs did, which is before any client that reads this, so it's the older one.
    pub fn older_side(&self) -> Option<OlderSide> {
        let ClientError::Api(ApiError { code: ErrorCode::VersionMismatch, hub, .. }) = self else { return None };
        Some(match hub {
            Some(hub) if hub.speaks.oldest > CLIENT_PROTOCOL_VERSION => {
                OlderSide::Client { hub_version: hub.version.clone() }
            }
            Some(hub) => OlderSide::Hub { version: Some(hub.version.clone()) },
            None => OlderSide::Hub { version: None },
        })
    }
}
