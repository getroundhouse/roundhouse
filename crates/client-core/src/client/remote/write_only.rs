//! A connection over the network that never reads what the hub sends, so a test can see what a hub does with a
//! device that stops reading. Only the `testing` feature has it.

use std::io::Write;

use protocol::client::{ClientFrame, ClientRequest};
use protocol::frame;

use super::reach;
use crate::client::transport::Closer;
use crate::client::{ClientError, Result};

pub struct WriteOnly {
    writer: Box<dyn Write + Send>,
    next_id: u64,
    _closer: Closer,
}

impl WriteOnly {
    /// Connects to the first of `hosts` to answer with the pinned key, and sends nothing yet.
    pub fn open(hosts: &[String], fingerprint: &str) -> Result<WriteOnly> {
        let transport = reach(hosts, fingerprint)?;
        Ok(WriteOnly { writer: transport.writer, next_id: 1, _closer: transport.closer })
    }

    /// Sends a request, and doesn't wait for its reply.
    pub fn send(&mut self, request: ClientRequest) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        let line = frame::encode(&ClientFrame { id, request }).map_err(|e| ClientError::Unexpected(e.to_string()))?;
        self.send_bytes(&line)
    }

    /// Sends whatever it's given, such as something that isn't a frame.
    pub fn send_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        Ok(self.writer.write_all(bytes)?)
    }
}
