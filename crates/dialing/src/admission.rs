//! What a worker says first on its connection, and what the hub answers, as `protocol::dialing` says. Each is one
//! frame, read a byte at a time, so nothing after it is read too: the stream goes on with the worker protocol.

use iroh::endpoint::{RecvStream, SendStream};
use protocol::frame;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::AsyncReadExt;

use crate::DialError;

/// The most either side's frame takes, far more than a secret or an instance's ID.
const MAX_BYTES: usize = 4096;

/// One frame of what the other side says, read a byte at a time.
pub async fn read_frame<T: DeserializeOwned>(recv: &mut RecvStream) -> Result<T, DialError> {
    let said = |e: &dyn std::fmt::Display| DialError::Said(e.to_string());
    let mut line = Vec::new();
    loop {
        let byte = recv.read_u8().await.map_err(|e| said(&e))?;
        line.push(byte);
        if byte == b'\n' {
            return frame::decode(&line).map_err(|e| said(&e));
        }
        if line.len() > MAX_BYTES {
            return Err(said(&"a frame longer than any it says"));
        }
    }
}

pub async fn write_frame<T: Serialize>(send: &mut SendStream, message: &T) -> Result<(), DialError> {
    let said = |e: &dyn std::fmt::Display| DialError::Said(e.to_string());
    send.write_all(&frame::encode(message).map_err(|e| said(&e))?).await.map_err(|e| said(&e))
}
