//! Listening on `127.0.0.1` for GitHub to send the person back: bound first, so the hub can be told where GitHub is to
//! send them in the manifest it writes, then serving the page that posts that manifest, until the code comes, the
//! listener is let go of, or its time limit passes, whichever is first.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use protocol::client::github_apps::GithubManifestCode;
use ring::rand::{SecureRandom as _, SystemRandom};

use super::answering::Serving;
use super::page;
use super::under::Under;

protocol::tokens!(
    /// What the page sends GitHub with the manifest, and GitHub sends back with the code, which no one else knows, so
    /// the listener takes a code only from the flow it started, rather than one another page sent the browser with.
    ManifestState,
);

/// Where the page that posts the manifest is, which the browser opens on, with the `state` after it.
pub(super) const MAKING: &str = "/make";
/// Where GitHub sends the person back, with the code and the `state` after it.
pub(super) const MADE: &str = "/made";

/// A listener bound to `127.0.0.1`, on a port the system picked, not serving yet.
pub(crate) struct Bound {
    listener: TcpListener,
    address: SocketAddr,
    state: ManifestState,
}

/// A listener serving the page, until it stops. Letting go of it stops it.
pub(crate) struct Listening {
    page_url: String,
    heard: flume::Receiver<Heard>,
    stop: flume::Sender<()>,
}

/// What the listener heard, once, before it stopped by itself.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Heard {
    /// GitHub sent the person back with this code, with the right `state`.
    Code(GithubManifestCode),
    /// Its time limit passed first.
    TimedOut,
}

impl Bound {
    /// Binds to `127.0.0.1` only, on a port the system picks, with a `state` of its own.
    pub(crate) fn new() -> io::Result<Bound> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        Ok(Bound { listener, address, state: new_state()? })
    }

    /// Where GitHub is to send the person back once they've made the app, for the hub's manifest to say.
    pub(crate) fn redirect_url(&self) -> String {
        format!("http://{}{MADE}", self.address)
    }

    /// Serves the page that posts `manifest` to GitHub's form for making an app `under` the account or organization
    /// chosen, and takes GitHub's redirect back, until it has the code, it's let go of, or `limit` has passed.
    pub(crate) fn serve(self, manifest: &serde_json::Value, under: &Under, limit: Duration) -> io::Result<Listening> {
        let Bound { listener, address, state } = self;
        let page_url = format!("http://{address}{MAKING}?state={}", state.as_str());
        let (heard_tx, heard) = flume::bounded(1);
        let (stop, stopped) = flume::bounded(1);
        let page = page::making(manifest, &under.form(), &state);
        let serving =
            Arc::new(Serving { state, page, ended: AtomicBool::new(false), heard: heard_tx, stop: stop.clone() });
        let accepting = Arc::clone(&serving);
        thread::Builder::new().name("github-app-listener".to_string()).spawn(move || accept(&listener, &accepting))?;
        thread::Builder::new()
            .name("github-app-listener-limit".to_string())
            .spawn(move || watch(&stopped, limit, &serving, address))?;
        Ok(Listening { page_url, heard, stop })
    }
}

impl Listening {
    /// The page the browser is to open on, which posts the manifest to GitHub.
    pub(crate) fn page_url(&self) -> &str {
        &self.page_url
    }

    /// Where what it hears comes, for a task to wait on. Nothing comes once it's been let go of.
    pub(crate) fn heard(&self) -> flume::Receiver<Heard> {
        self.heard.clone()
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        // Full only once the code has stopped it already.
        let _ = self.stop.try_send(());
    }
}

/// Answers each connection on a thread of its own, so one the browser opens ahead and sends nothing on holds up none
/// of the others, until the listener has ended, then stops listening.
fn accept(listener: &TcpListener, serving: &Arc<Serving>) {
    for stream in listener.incoming() {
        if serving.ended.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let serving = Arc::clone(serving);
        // One that can't be answered is let go of, as a browser would see a busy server do.
        let _ =
            thread::Builder::new().name("github-app-listener-answer".to_string()).spawn(move || serving.answer(stream));
    }
}

/// Ends the listener once it's stopped, or once `limit` has passed, saying it timed out unless something ended it
/// first, then wakes the listener at `address` so it sees it has ended.
fn watch(stopped: &flume::Receiver<()>, limit: Duration, serving: &Serving, address: SocketAddr) {
    let timed_out = matches!(stopped.recv_timeout(limit), Err(flume::RecvTimeoutError::Timeout));
    if !serving.ended.swap(true, Ordering::SeqCst) && timed_out {
        let _ = serving.heard.try_send(Heard::TimedOut);
    }
    // Refused once the listener has gone, which is fine, since there's nothing left to wake.
    let _ = TcpStream::connect(address);
}

/// The digits a state is written in.
const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 128 bits from the system's source of randomness, as hex.
fn new_state() -> io::Result<ManifestState> {
    let mut bytes = [0_u8; 16];
    SystemRandom::new().fill(&mut bytes).map_err(|_| io::Error::other("the system gave no randomness"))?;
    let hex = bytes.iter().flat_map(|byte| [DIGITS[usize::from(byte >> 4)], DIGITS[usize::from(byte & 0xf)]]);
    Ok(ManifestState::new(hex.map(char::from).collect::<String>()))
}
