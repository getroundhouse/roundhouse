//! Answering one connection to the listener: what it asks for is read, and answered as the caller says. Only a `GET` is
//! answered that way, and for anything else there's nothing here.

use std::io::{Read as _, Write as _};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use url::form_urlencoded;

use super::listening::Ended;
use super::page::{self, Status};

/// How long a connection has to send its request, so one that sends nothing doesn't keep its thread for long.
const READ_LIMIT: Duration = Duration::from_secs(10);

/// The most of a request read, which a browser's address and headers fit in many times over.
const LONGEST_HEAD: usize = 16 * 1024;

protocol::tokens!(
    /// The query of an address the browser was sent to, as it came, which can hold a code that's as good as a login.
    Query,
);

/// What a `GET` asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Asked {
    pub(crate) path: String,
    pub(crate) query: Query,
}

/// How a request is answered.
pub(crate) enum Answer<H> {
    /// With this page, and the listener goes on.
    Page(Status, String),
    /// It's what was listened for, which ends the listener, and it's answered with the page `taken`, or with `late` once
    /// the listener has ended already.
    Heard { heard: H, taken: String, late: String },
}

impl<H> Answer<H> {
    /// How a request at a path the caller has nothing at is answered.
    pub(crate) fn nothing_here() -> Self {
        Answer::Page(Status::NotFound, page::nothing_here())
    }
}

/// What each request is answered by.
pub(super) type Answers<H> = Box<dyn Fn(&Asked) -> Answer<H> + Send + Sync>;

/// What every connection is answered from.
pub(super) struct Serving<H> {
    pub(super) answers: Answers<H>,
    /// Whether the listener has ended: it heard what it listened for, it was let go of, or its limit passed.
    pub(super) ended: AtomicBool,
    pub(super) heard: flume::Sender<Ended<H>>,
    /// Ends the wait for the limit.
    pub(super) stop: flume::Sender<()>,
}

impl<H> Serving<H> {
    /// Reads what `stream` asks, and answers it, then closes it.
    pub(super) fn answer(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(READ_LIMIT));
        let Some(line) = request_line(&mut stream) else { return };
        let answer = Asked::of(&line).map_or_else(Answer::nothing_here, |asked| (self.answers)(&asked));
        let answered = self.answered(answer);
        let _ = stream.write_all(answered.as_bytes());
        let _ = stream.shutdown(Shutdown::Write);
    }

    /// The response `answer` is, having ended the listener where it's what was listened for.
    fn answered(&self, answer: Answer<H>) -> String {
        match answer {
            Answer::Page(status, page) => page::answer(status, &page),
            Answer::Heard { late, .. } if self.ended.swap(true, Ordering::SeqCst) => page::answer(Status::Gone, &late),
            Answer::Heard { heard, taken, .. } => {
                let _ = self.heard.try_send(Ended::Heard(heard));
                let _ = self.stop.try_send(());
                page::answer(Status::Ok, &taken)
            }
        }
    }
}

impl Asked {
    /// What the request line `line`, such as `GET /made?code=…&state=… HTTP/1.1`, asks for, if it's a `GET`.
    pub(super) fn of(line: &str) -> Option<Asked> {
        let mut parts = line.split(' ');
        let (Some("GET"), Some(target), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        if !version.starts_with("HTTP/1.") {
            return None;
        }
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        Some(Asked { path: path.to_string(), query: Query::new(query) })
    }
}

impl Query {
    /// What it says `name` is, as a form's encoding has it, or `None` where it says that twice, or not at all.
    pub(crate) fn value(&self, name: &str) -> Option<String> {
        let mut said = form_urlencoded::parse(self.as_str().as_bytes()).filter(|(named, _)| named == name);
        let (_, value) = said.next()?;
        said.next().is_none().then(|| value.into_owned())
    }
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
