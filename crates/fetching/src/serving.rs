//! A web server on this computer for tests, standing in for where files are published. It answers each path as it's
//! told, each request on a thread of its own, and keeps what it was asked for.

use std::io::{self, Read as _, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use parking_lot::{Condvar, Mutex};

/// One answer to a request.
#[derive(Clone)]
pub struct Reply {
    status: &'static str,
    headers: Vec<String>,
    body: Vec<u8>,
    /// What it says its body's length is, if not the body's own: longer, for a body cut off in transit.
    claimed_bytes: Option<usize>,
    /// Sent up to this many bytes of the body, then held until the test lets it go.
    held: Option<(usize, Release)>,
}

impl Reply {
    pub fn file(body: impl Into<Vec<u8>>) -> Self {
        Self::new("200 OK", Vec::new(), body.into())
    }

    /// The file compressed in transit, which the client decompresses on its own.
    pub fn gzipped(body: &[u8]) -> io::Result<Self> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(body)?;
        Ok(Self::new("200 OK", vec!["Content-Encoding: gzip".to_string()], encoder.finish()?))
    }

    pub fn redirect(to: &str) -> Self {
        Self::new("302 Found", vec![format!("Location: {to}")], Vec::new())
    }

    /// Part of a file, as an answer to a request for a range of it.
    pub fn partial(body: impl Into<Vec<u8>>) -> Self {
        Self::new("206 Partial Content", Vec::new(), body.into())
    }

    /// A file that says it's `claimed_bytes` long, and is cut off after `body`, as a dropped connection would be.
    pub fn cut_off(body: impl Into<Vec<u8>>, claimed_bytes: usize) -> Self {
        Self { claimed_bytes: Some(claimed_bytes), ..Self::file(body) }
    }

    /// This reply, sent up to `after` bytes of its body, then held until the test lets it go with what this returns.
    pub fn held_after(mut self, after: usize) -> (Self, Release) {
        let release = Release::default();
        self.held = Some((after, release.clone()));
        (self, release)
    }

    fn new(status: &'static str, headers: Vec<String>, body: Vec<u8>) -> Self {
        Self { status, headers, body, claimed_bytes: None, held: None }
    }
}

/// Lets a held reply go on.
#[derive(Clone, Default)]
pub struct Release(Arc<(Mutex<bool>, Condvar)>);

impl Release {
    pub fn now(&self) {
        let (released, changed) = &*self.0;
        *released.lock() = true;
        changed.notify_all();
    }

    fn wait(&self) {
        let (released, changed) = &*self.0;
        let mut released = released.lock();
        while !*released {
            changed.wait(&mut released);
        }
    }
}

/// What happened at the server, in the order it did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A request for this path came.
    Asked(String),
    /// The reply to a request for this path was let go, and the rest of it is on its way.
    Released(String),
}

pub struct Server {
    pub base: String,
    events: Arc<Mutex<Vec<Event>>>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Server {
    /// The paths asked for, in order.
    pub fn asked(&self) -> Vec<String> {
        let asked = |event: &Event| match event {
            Event::Asked(path) => Some(path.clone()),
            Event::Released(_) => None,
        };
        self.events.lock().iter().filter_map(asked).collect()
    }

    /// Each request and each release, in order.
    pub fn events(&self) -> Vec<Event> {
        self.events.lock().clone()
    }

    /// Each request whole, its headers too, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().clone()
    }
}

/// Serves each path's replies in turn, then its last one from then on, and a 404 for any other path, until the test
/// process ends. A connection it can't read from is left unanswered.
pub fn serve(paths: Vec<(String, Vec<Reply>)>) -> io::Result<Server> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    let (events, requests) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
    let (log, whole) = (Arc::clone(&events), Arc::clone(&requests));
    std::thread::spawn(move || {
        let answered = Mutex::new(vec![0; paths.len()]);
        let next = |path: &str| match paths.iter().position(|(served, _)| served == path) {
            Some(at) => {
                let replies = &paths[at].1;
                let mut answered = answered.lock();
                answered[at] += 1;
                replies[(answered[at] - 1).min(replies.len() - 1)].clone()
            }
            None => Reply::new("404 Not Found", Vec::new(), Vec::new()),
        };
        for mut stream in listener.incoming().flatten() {
            let Ok(request) = read_request(&mut stream) else { continue };
            let target = request.split_whitespace().nth(1).unwrap_or_default();
            let path = target.trim_start_matches('/').split('?').next().unwrap_or_default().to_string();
            let reply = next(&path);
            log.lock().push(Event::Asked(path.clone()));
            whole.lock().push(request);
            let log = Arc::clone(&log);
            // The client may hang up as soon as it has read enough.
            std::thread::spawn(move || answer(&mut stream, &reply, || log.lock().push(Event::Released(path))));
        }
    });
    Ok(Server { base, events, requests })
}

/// A request, up to the end of its headers, however many reads that takes.
fn read_request(stream: &mut TcpStream) -> io::Result<String> {
    let (mut request, mut buffer) = (Vec::new(), [0; 4096]);
    while !request.ends_with(b"\r\n\r\n") {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    Ok(String::from_utf8_lossy(&request).to_string())
}

fn answer(stream: &mut TcpStream, reply: &Reply, released: impl FnOnce()) -> io::Result<()> {
    let length = reply.claimed_bytes.unwrap_or(reply.body.len());
    write!(stream, "HTTP/1.1 {}\r\nContent-Length: {length}\r\nConnection: close\r\n", reply.status)?;
    for header in &reply.headers {
        write!(stream, "{header}\r\n")?;
    }
    stream.write_all(b"\r\n")?;
    let Some((after, release)) = &reply.held else { return stream.write_all(&reply.body) };
    let after = (*after).min(reply.body.len());
    stream.write_all(&reply.body[..after])?;
    stream.flush()?;
    release.wait();
    released();
    stream.write_all(&reply.body[after..])
}
