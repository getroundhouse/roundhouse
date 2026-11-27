//! Requests that only a client on the hub's machine may make.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::devices;
use protocol::model::DeviceId;
use protocol::pairing::{Device, Pairing, PairingCode};

use super::{Connection, Result};

impl Connection {
    pub fn pairing_start(&self) -> Result<Pairing> {
        self.call(devices::PairingStart)
    }

    pub fn pairing_wait(&self, code: &PairingCode) -> Result<Device> {
        self.call(devices::PairingWait { code: code.clone() })
    }

    pub fn device_list(&self) -> Result<Vec<Device>> {
        Ok(self.call(devices::DeviceList)?.devices)
    }

    pub fn device_revoke(&self, id: &DeviceId) -> Result<()> {
        self.call(devices::DeviceRevoke { id: id.clone() })
    }
}

enum Slot {
    Connecting,
    Waiting(Connection),
    Cancelled,
}

/// Waits on a local connection of its own for a device to claim a pairing code. Dropping it closes that connection,
/// which ends both the waiting thread and the hub's wait without touching any other client.
pub struct PairingWait {
    paired: flume::Receiver<Result<Device>>,
    slot: Arc<Mutex<Slot>>,
}

impl PairingWait {
    pub fn start(socket: PathBuf, code: PairingCode) -> Self {
        let (tx, paired) = flume::bounded(1);
        let slot = Arc::new(Mutex::new(Slot::Connecting));
        let shared = slot.clone();
        let on_spawn_error = tx.clone();
        let spawned = std::thread::Builder::new().name("pairing-wait".to_string()).spawn(move || {
            let connection = match Connection::connect(&socket, "pairing-wait") {
                Ok(connection) => connection,
                Err(err) => {
                    let _ = tx.send(Err(err));
                    return;
                }
            };
            {
                let mut guard = shared.lock();
                if matches!(*guard, Slot::Cancelled) {
                    connection.disconnect();
                    return;
                }
                *guard = Slot::Waiting(connection.clone());
            }
            let _ = tx.send(connection.pairing_wait(&code));
        });
        if let Err(err) = spawned {
            let _ = on_spawn_error.send(Err(err.into()));
        }
        Self { paired, slot }
    }

    pub fn recv(&self) -> &flume::Receiver<Result<Device>> {
        &self.paired
    }
}

impl Drop for PairingWait {
    fn drop(&mut self) {
        let previous = std::mem::replace(&mut *self.slot.lock(), Slot::Cancelled);
        if let Slot::Waiting(connection) = previous {
            connection.disconnect();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::os::unix::net::UnixListener;
    use std::time::Duration;

    use protocol::client::{
        CLIENT_PROTOCOL_VERSION, ClientFrame, ClientRequest, ClientResponse, HelloReply, HubFrame, ReplyOutcome,
    };
    use protocol::frame;

    use super::*;

    #[test]
    fn dropping_pairing_wait_closes_its_connection_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("hub.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let wait = PairingWait::start(socket, PairingCode::from("code-123"));
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);

        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).unwrap();
        let hello: ClientFrame = frame::decode(&line).unwrap();
        assert!(matches!(hello.request, ClientRequest::Hello(_)));
        let reply = HubFrame::Reply {
            id: hello.id,
            outcome: ReplyOutcome::Response(ClientResponse::Hello(HelloReply {
                protocol_version: CLIENT_PROTOCOL_VERSION,
                hub_version: "1.0.0".to_string(),
            })),
        };
        writer.write_all(&frame::encode(&reply).unwrap()).unwrap();

        line.clear();
        reader.read_until(b'\n', &mut line).unwrap();
        let waiting: ClientFrame = frame::decode(&line).unwrap();
        assert!(matches!(waiting.request, ClientRequest::PairingWait(_)));

        // Leaving `PairingWait` unanswered, dropping `wait` disconnects its dedicated socket immediately.
        let paired_rx = wait.recv().clone();
        drop(wait);

        line.clear();
        let bytes = reader.read_until(b'\n', &mut line).unwrap();
        assert_eq!(bytes, 0, "the waiting socket closed as soon as PairingWait dropped");
        assert!(paired_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_err());
    }
}
