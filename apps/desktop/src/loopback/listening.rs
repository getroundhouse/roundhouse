//! Serving, until what's listened for comes, the listener is let go of, or its time limit passes, whichever is first.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use super::answering::{Answers, Serving};

/// A listener serving, until it stops. Letting go of it stops it.
pub(crate) struct Listening<H> {
    ended: flume::Receiver<Ended<H>>,
    stop: flume::Sender<()>,
}

/// How a listener stopped by itself.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ended<H> {
    /// What it listened for came.
    Heard(H),
    /// Its time limit passed first.
    TimedOut,
}

impl<H> Listening<H> {
    /// Where how it ended comes, once, for a task to wait on. Nothing comes once it's been let go of.
    pub(crate) fn ended(&self) -> flume::Receiver<Ended<H>> {
        self.ended.clone()
    }
}

impl<H> Drop for Listening<H> {
    fn drop(&mut self) {
        // Full only once what it listened for has stopped it already.
        let _ = self.stop.try_send(());
    }
}

/// Serves on `listener`, which is at `address`.
pub(super) fn start<H: Send + 'static>(
    listener: TcpListener,
    address: SocketAddr,
    answers: Answers<H>,
    limit: Duration,
) -> io::Result<Listening<H>> {
    let (heard, ended) = flume::bounded(1);
    let (stop, stopped) = flume::bounded(1);
    let serving = Arc::new(Serving { answers, ended: AtomicBool::new(false), heard, stop: stop.clone() });
    let accepting = Arc::clone(&serving);
    thread::Builder::new().name("loopback".to_string()).spawn(move || accept(&listener, &accepting))?;
    thread::Builder::new()
        .name("loopback-limit".to_string())
        .spawn(move || watch(&stopped, limit, &serving, address))?;
    Ok(Listening { ended, stop })
}

/// Answers each connection on a thread of its own, so one the browser opens ahead and sends nothing on holds up none
/// of the others, until the listener has ended, then stops listening.
fn accept<H: Send + 'static>(listener: &TcpListener, serving: &Arc<Serving<H>>) {
    for stream in listener.incoming() {
        if serving.ended.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let serving = Arc::clone(serving);
        // One that can't be answered is let go of, as a browser would see a busy server do.
        let _ = thread::Builder::new().name("loopback-answer".to_string()).spawn(move || serving.answer(stream));
    }
}

/// Ends the listener once it's stopped, or once `limit` has passed, saying it timed out unless something ended it
/// first, then wakes the listener at `address` so it sees it has ended.
fn watch<H>(stopped: &flume::Receiver<()>, limit: Duration, serving: &Serving<H>, address: SocketAddr) {
    let timed_out = matches!(stopped.recv_timeout(limit), Err(flume::RecvTimeoutError::Timeout));
    if !serving.ended.swap(true, Ordering::SeqCst) && timed_out {
        let _ = serving.heard.try_send(Ended::TimedOut);
    }
    // Refused once the listener has gone, which is fine, since there's nothing left to wake.
    let _ = TcpStream::connect(address);
}
