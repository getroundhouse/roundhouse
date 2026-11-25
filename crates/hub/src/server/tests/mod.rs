//! What a hub does with what a connection sends it, over a stream in memory rather than its socket.

mod connection;
mod greeting;
mod logged;
mod revised;
mod tool_servers;

pub(super) use connection::{Connection, WAIT};
