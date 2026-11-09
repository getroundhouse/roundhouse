//! Calls that change something, each started at once on a thread of its own, so it runs to the end whatever its
//! caller does next, even quitting. A client about to exit waits for them with [`Commands::finished`].

use std::sync::Arc;

use parking_lot::Mutex;

use super::{Client, ClientError, Result};

/// How many started calls are under way, and who's waiting for there to be none.
#[derive(Default)]
pub(super) struct Running {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    count: usize,
    waiting: Vec<flume::Sender<()>>,
}

impl Running {
    fn begin(&self) {
        self.state.lock().count += 1;
    }

    fn end(&self) {
        let waiting = {
            let mut state = self.state.lock();
            state.count = state.count.saturating_sub(1);
            if state.count > 0 {
                return;
            }
            std::mem::take(&mut state.waiting)
        };
        for done in waiting {
            let _ = done.send(());
        }
    }

    fn finished(&self) -> flume::Receiver<()> {
        let (done, finished) = flume::bounded(1);
        let mut state = self.state.lock();
        if state.count == 0 {
            let _ = done.send(());
        } else {
            state.waiting.push(done);
        }
        finished
    }
}

/// The calls started on one connection, for waiting until none is under way, without keeping the connection open.
#[derive(Clone)]
pub struct Commands(Arc<Running>);

impl Commands {
    /// Hears once no call started with [`Client::start`] is under way, at once if none is.
    pub fn finished(&self) -> flume::Receiver<()> {
        self.0.finished()
    }
}

/// Ends a started call however its thread ends, even by panicking, or by never starting.
struct Ending(Client);

impl Drop for Ending {
    fn drop(&mut self) {
        self.0.inner.running.end();
    }
}

/// The reply to a call started with [`Client::start`].
pub struct Reply<T>(flume::Receiver<Result<T>>);

impl<T> Reply<T> {
    /// Waits for the reply without holding a thread. A call that ended without one, which only a panic does, counts as
    /// the connection lost.
    pub async fn recv(self) -> Result<T> {
        self.0.recv_async().await.unwrap_or(Err(ClientError::Disconnected))
    }
}

impl Client {
    /// Starts a call that changes something, on a thread of its own, and returns at once, so the caller never waits
    /// on the hub. The call runs to the end whatever the caller does next, and [`Commands::finished`] waits for it.
    pub fn start<T: Send + 'static>(&self, call: impl FnOnce(&Client) -> Result<T> + Send + 'static) -> Reply<T> {
        let (reply, replied) = flume::bounded(1);
        let failed = reply.clone();
        // Counted before this returns, so a `finished` asked for after it always waits for it.
        self.inner.running.begin();
        let ending = Ending(self.clone());
        let spawned = std::thread::Builder::new().name("hub-client-call".to_string()).spawn(move || {
            let _ = reply.send(call(&ending.0));
            drop(ending);
        });
        if let Err(e) = spawned {
            let _ = failed.send(Err(ClientError::Io(e)));
        }
        Reply(replied)
    }

    /// The calls started on this connection, which can be waited for after it's gone.
    pub fn commands(&self) -> Commands {
        Commands(self.inner.running.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::path::Path;
    use std::time::Duration;

    use protocol::PROTOCOL_VERSION;
    use protocol::client::{ClientResponse, HubFrame, ReplyOutcome};
    use protocol::frame;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(10);

    /// A client of a hub that only says hello, which is all these calls need.
    fn connected(socket: &Path) -> Client {
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut requests = BufReader::new(stream.try_clone().unwrap()).lines();
            requests.next();
            let hello = ClientResponse::Hello { protocol_version: PROTOCOL_VERSION, hub_version: "a".to_string() };
            let outcome = ReplyOutcome::Response(hello);
            stream.write_all(&frame::encode(&HubFrame::Reply { id: 1, outcome }).unwrap()).unwrap();
            // Stays connected for as long as the client is.
            requests.next();
        });
        Client::connect(socket, "test").unwrap()
    }

    #[test]
    fn finished_answers_at_once_when_nothing_is_under_way() {
        let dir = tempfile::tempdir().unwrap();
        let client = connected(&dir.path().join("hub.sock"));

        assert!(client.commands().finished().try_recv().is_ok());
    }

    #[test]
    fn finished_waits_for_a_call_started_before_it_and_its_reply_arrives() {
        let dir = tempfile::tempdir().unwrap();
        let client = connected(&dir.path().join("hub.sock"));
        let (go, wait) = flume::bounded::<()>(1);

        let reply = client.start(move |_| {
            let _ = wait.recv();
            Ok(5)
        });
        let finished = client.commands().finished();

        assert!(finished.try_recv().is_err(), "the call is still under way");
        go.send(()).unwrap();
        finished.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(reply.0.recv_timeout(TIMEOUT).unwrap().unwrap(), 5);
    }

    #[test]
    fn a_call_that_panics_still_ends_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let client = connected(&dir.path().join("hub.sock"));

        let reply = client.start(|_| -> Result<()> { panic!("a bug in the call") });

        client.commands().finished().recv_timeout(TIMEOUT).unwrap();
        assert!(reply.0.recv_timeout(TIMEOUT).is_err(), "no reply, which `recv` reports as the connection lost");
    }
}
