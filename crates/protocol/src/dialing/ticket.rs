use std::fmt;
use std::net::SocketAddr;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

/// What a worker dials its hub with: where the hub's endpoint is, and the secret that admits one worker, once. It names
/// the hub's relay and its direct addresses, so neither side publishes its addresses to n0's address lookup, and the
/// worker needs nothing but the ticket to find the hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub hub: HubAddress,
    pub secret: TicketSecret,
}

/// Where a hub's iroh endpoint is, as it was when the ticket was made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubAddress {
    /// The endpoint's public key, as iroh writes it, which is what a connection to it proves.
    pub key: String,
    /// The URL of its home relay, if it uses one. A worker given none dials the direct addresses only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    /// The addresses it can be reached at directly, such as on its own network.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct: Vec<SocketAddr>,
}

/// The part of a ticket that admits a worker, which only the hub, the provider it was given to and that worker hold.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct TicketSecret(String);

impl TicketSecret {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for TicketSecret {
    fn from(secret: String) -> Self {
        Self(secret)
    }
}

/// Never the secret itself, so a log of what holds one doesn't hold it.
impl fmt::Debug for TicketSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TicketSecret(..)")
    }
}

/// A [`Ticket`] as it's given to a provider and passed to its worker, as one word that a command line, an environment
/// variable or a file holds as it is: its JSON, in unpadded base64url. An adapter only passes it on, so it can't depend
/// on what's in it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct WorkerTicket(String);

impl WorkerTicket {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn read(&self) -> Result<Ticket, TicketError> {
        let json = URL_SAFE_NO_PAD.decode(self.0.trim()).map_err(|_| TicketError::NotBase64)?;
        serde_json::from_slice(&json).map_err(|e| TicketError::NotATicket(e.to_string()))
    }
}

impl From<&Ticket> for WorkerTicket {
    #[expect(clippy::expect_used, reason = "a ticket is strings and addresses, which always serialize")]
    fn from(ticket: &Ticket) -> Self {
        Self(URL_SAFE_NO_PAD.encode(serde_json::to_vec(ticket).expect("a ticket serializes")))
    }
}

impl From<String> for WorkerTicket {
    fn from(ticket: String) -> Self {
        Self(ticket)
    }
}

/// Never the ticket itself, which holds its secret.
impl fmt::Debug for WorkerTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkerTicket(..)")
    }
}

/// Why a [`WorkerTicket`] couldn't be read.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum TicketError {
    #[error("the ticket isn't base64url")]
    NotBase64,
    #[error("the ticket doesn't hold a ticket: {0}")]
    NotATicket(String),
}
