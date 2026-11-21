//! A stand-in for a client connected to a hub, and a paused clock to let what the hub does meanwhile all be done.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::{CLIENT_PROTOCOL_VERSION, ClientFrame, ClientRequest, Hello, HubFrame};
use protocol::frame;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};
use tokio::time::timeout;

use crate::hub::Hub;

/// Long enough for anything here on a busy machine.
pub(super) const WAIT: Duration = Duration::from_secs(10);

/// Until everything else is waiting. Only a moment on tokio's paused clock, where a sleep lasts until everything else
/// is waiting, so the hub's own timers, of seconds, don't pass.
#[expect(
    clippy::disallowed_methods,
    reason = "on tokio's paused clock, which moves on once everything else is waiting"
)]
pub(super) async fn settled() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

/// A client's end of a connection to the hub, over a buffer of `buffer` bytes each way, which has said hello.
pub(super) struct Connected {
    pub(super) frames: Lines<BufReader<ReadHalf<DuplexStream>>>,
    pub(super) requests: WriteHalf<DuplexStream>,
}

impl Connected {
    pub(super) async fn to(hub: &Arc<Hub>, buffer: usize) -> Self {
        let (client, served) = tokio::io::duplex(buffer);
        let (read, write) = tokio::io::split(served);
        let gate = crate::server::admit::Gate::local(hub);
        tokio::spawn(crate::server::connection::serve_connection(hub.clone(), read, write, gate));
        let (read, requests) = tokio::io::split(client);
        let mut connected = Self { frames: BufReader::new(read).lines(), requests };
        let hello = ClientRequest::Hello(Hello {
            protocol_version: CLIENT_PROTOCOL_VERSION,
            client: "t".into(),
            oldest_protocol_version: None,
            device: None,
        });
        connected.send(0, hello).await;
        connected.next().await;
        connected
    }

    pub(super) async fn send(&mut self, id: u64, request: ClientRequest) {
        self.requests.write_all(&frame::encode(&ClientFrame { id, request }).unwrap()).await.unwrap();
    }

    pub(super) async fn next(&mut self) -> HubFrame {
        let line = timeout(WAIT, self.frames.next_line()).await.expect("a frame").unwrap().expect("the connection");
        frame::decode(line.as_bytes()).unwrap()
    }
}
