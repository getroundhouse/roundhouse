//! The client side of the hub: connections, requests and subscriptions.

mod pump;
mod subscribing;
mod terminals;

mod connection;
use connection::{Connection, Outgoing};
pub(crate) use connection::{PUMP_PAGE_EVENTS, accept_loop};

mod admit;
mod counted;
mod dispatch;
mod hello;
mod network;
pub(crate) use network::accept_devices;

#[cfg(test)]
mod tests;
