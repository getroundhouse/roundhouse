//! Why a hub connection failed, distinguishing local failures from remote network, key and authorization failures.

use client_core::ClientError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionFailure {
    /// Local hub couldn't be reached or started, or following failed.
    Local(String),
    /// Remote hub couldn't be reached over the network; retried automatically.
    Unreachable { hosts: Vec<String> },
    /// Remote hub rejected our saved device token.
    Revoked,
    /// Remote hub answered with a different TLS key than the pinned one.
    KeyChanged { host: String },
    /// Remote hub speaks an incompatible protocol version.
    RemoteProtocolBreak(String),
    /// Any other failure talking to a remote hub.
    RemoteOther(String),
}

impl ConnectionFailure {
    pub(super) fn of(local: bool, error: &ClientError) -> Self {
        if local {
            return Self::Local(error.to_string());
        }
        match error {
            ClientError::Unreachable { hosts, .. } => Self::Unreachable { hosts: hosts.clone() },
            ClientError::Io(_) | ClientError::Disconnected => Self::Unreachable { hosts: Vec::new() },
            ClientError::FingerprintMismatch { host } => Self::KeyChanged { host: host.clone() },
            ClientError::Api(_) if error.is_unauthorized() => Self::Revoked,
            ClientError::Api(api) if error.is_version_mismatch() => Self::RemoteProtocolBreak(api.message.clone()),
            ClientError::Api(api) => Self::RemoteOther(api.message.clone()),
            ClientError::Unexpected(detail) => Self::RemoteOther(detail.clone()),
        }
    }

    pub(super) fn of_follow(local: bool, message: String) -> Self {
        if local { Self::Local(message) } else { Self::RemoteOther(message) }
    }

    pub(crate) fn switcher_label(&self) -> &'static str {
        match self {
            Self::Local(_) | Self::Unreachable { .. } | Self::RemoteOther(_) => "Offline",
            Self::Revoked => "Access revoked",
            Self::KeyChanged { .. } => "Key changed",
            Self::RemoteProtocolBreak(_) => "Update required",
        }
    }

    pub(crate) fn needs_attention(&self) -> bool {
        matches!(self, Self::Revoked | Self::KeyChanged { .. } | Self::RemoteProtocolBreak(_))
    }
}
