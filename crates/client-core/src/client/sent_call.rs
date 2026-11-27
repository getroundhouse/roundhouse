//! A call sent to the hub whose reply is waited for later, maybe on another thread.

use std::marker::PhantomData;

use protocol::client::{ClientResponse, Reply as _, Request};

use super::{ClientError, Connection, Result};

/// A call that has been sent, whose reply [`SentCall::reply`] waits for.
pub(in crate::client) struct SentCall<R> {
    reply: flume::Receiver<Result<ClientResponse>>,
    connection: Connection,
    request: PhantomData<fn() -> R>,
}

impl<R: Request> SentCall<R> {
    pub(super) fn new(reply: flume::Receiver<Result<ClientResponse>>, connection: Connection) -> Self {
        Self { reply, connection, request: PhantomData }
    }

    /// Waits for the reply, read as [`Connection::call`] reads it.
    pub(in crate::client) fn reply(self) -> Result<R::Reply> {
        let response = self.reply.recv().map_err(|_| ClientError::Disconnected)??;
        self.connection.inner.folding.fold(&response);
        R::Reply::read(response).map_err(|response| ClientError::Unexpected(format!("{response:?}")))
    }
}
