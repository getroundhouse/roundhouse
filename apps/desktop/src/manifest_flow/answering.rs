//! Answering one connection to the listener: the page that posts the manifest, at its path with the right `state`,
//! and the code GitHub sends the person back with, at its own, with the same `state`, which ends the listener. Every
//! other path, method or `state` is refused.

use std::io::{Read as _, Write as _};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use protocol::client::github_apps::GithubManifestCode;

use super::listening::{Heard, MADE, MAKING, ManifestState};
use super::page::{self, Status};

/// How long a connection has to send its request, so one that sends nothing doesn't keep its thread for long.
const READ_LIMIT: Duration = Duration::from_secs(10);

/// The most of a request read, which a browser's address and headers fit in many times over.
const LONGEST_HEAD: usize = 16 * 1024;

/// What every connection is answered from.
pub(super) struct Serving {
    pub(super) state: ManifestState,
    /// The page that posts the manifest, whole.
    pub(super) page: String,
    /// Whether the listener has ended: it has the code, it was let go of, or its limit passed.
    pub(super) ended: AtomicBool,
    pub(super) heard: flume::Sender<Heard>,
    /// Ends the wait for the limit.
    pub(super) stop: flume::Sender<()>,
}

/// What a request asks for.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Asked {
    /// The page that posts the manifest, with the `state` it says, if it says one.
    Making { state: Option<String> },
    /// What GitHub sends the person back to, with the code and `state` it says, if it says them.
    Made { code: Option<GithubManifestCode>, state: Option<String> },
    /// Anything else, which is refused.
    Elsewhere,
}

impl Serving {
    /// Reads what `stream` asks, and answers it, then closes it.
    pub(super) fn answer(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(READ_LIMIT));
        let Some(line) = request_line(&mut stream) else { return };
        let answer = self.answer_to(&Asked::of(&line));
        let _ = stream.write_all(answer.as_bytes());
        let _ = stream.shutdown(Shutdown::Write);
    }

    fn answer_to(&self, asked: &Asked) -> String {
        let ours = |state: &Option<String>| state.as_deref() == Some(self.state.as_str());
        match asked {
            Asked::Making { state } if ours(state) => page::answer(Status::Ok, &self.page),
            Asked::Made { code: Some(code), state } if ours(state) => self.took(code),
            Asked::Making { .. } | Asked::Made { .. } => page::answer(Status::BadRequest, &page::said(page::NOT_OURS)),
            Asked::Elsewhere => page::answer(Status::NotFound, &page::said(page::NOT_HERE)),
        }
    }

    /// Takes `code`, unless the listener has ended already, and ends it.
    fn took(&self, code: &GithubManifestCode) -> String {
        if self.ended.swap(true, Ordering::SeqCst) {
            return page::answer(Status::Gone, &page::said(page::STOPPED));
        }
        let _ = self.heard.try_send(Heard::Code(code.clone()));
        let _ = self.stop.try_send(());
        page::answer(Status::Ok, &page::said(page::BACK))
    }
}

impl Asked {
    /// What the request line `line`, such as `GET /made?code=…&state=… HTTP/1.1`, asks for.
    pub(super) fn of(line: &str) -> Asked {
        let mut parts = line.split(' ');
        let (Some("GET"), Some(target), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Asked::Elsewhere;
        };
        if !version.starts_with("HTTP/1.") {
            return Asked::Elsewhere;
        }
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let value = |key: &str| {
            let pairs = query.split('&').filter_map(|pair| pair.split_once('='));
            pairs.filter(|(named, _)| *named == key).find_map(|(_, value)| decoded(value))
        };
        match path {
            MAKING => Asked::Making { state: value("state") },
            MADE => Asked::Made { code: value("code").map(GithubManifestCode::new), state: value("state") },
            _ => Asked::Elsewhere,
        }
    }
}

/// A value from a query, as a form's encoding has it, with `+` for a space.
fn decoded(value: &str) -> Option<String> {
    let value = value.replace('+', " ");
    percent_encoding::percent_decode_str(&value).decode_utf8().ok().map(std::borrow::Cow::into_owned)
}

/// The first line of the request `stream` sends, once its head has come, so nothing it sent is left unread when it's
/// answered and closed, which would have the system reset the connection rather than let the browser read the answer.
/// `None` when nothing came in time, or the head is too long.
fn request_line(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !head.windows(4).any(|end| end == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).ok().filter(|read| *read > 0)?;
        head.extend_from_slice(&chunk[..read]);
        if head.len() > LONGEST_HEAD {
            return None;
        }
    }
    let head = String::from_utf8_lossy(&head);
    head.split("\r\n").next().map(str::to_string)
}
