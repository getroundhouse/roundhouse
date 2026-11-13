//! The hub's side of the provider protocol. Runs a provider's adapter as a child of the hub, and asks it what
//! [`Provider`](providers::Provider) asks, so the rest of the hub treats it as it does any provider. Every request is safe to repeat,
//! so one whose adapter exits before answering is asked again once, of a new one.

#[cfg(test)]
use parking_lot::Mutex;
#[cfg(test)]
use protocol::model::InstanceId;
#[cfg(test)]
use protocol::provider::{HelloReply, ProviderFrame, ProviderOutcome, ProviderResponse};
#[cfg(test)]
use protocol::{Build, frame};
#[cfg(test)]
use providers::{Capabilities, Provider, ProviderError};
#[cfg(test)]
use std::io;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
mod tests;

mod connection;

#[cfg(test)]
use connection::REQUEST_TIMEOUT;
pub(crate) use connection::{ADAPTER_GRACE, AdapterCommand, AdapterProvider, process};
#[cfg(test)]
pub(crate) use connection::{Open, Opened};
