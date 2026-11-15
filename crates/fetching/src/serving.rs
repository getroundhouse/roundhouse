//! A web server on this computer for tests, standing in for where files are published. It answers each path as it's
//! told, and keeps the paths it was asked for.

use std::io::{self, Read as _, Write};
use std::net::TcpListener;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use parking_lot::Mutex;

/// One answer to a request.
#[derive(Clone)]
pub struct Reply {
    status: &'static str,
    headers: Vec<String>,
    body: Vec<u8>,
    /// What it says its body's length is, if not the body's own: longer, for a body cut off in transit.
    claimed_bytes: Option<usize>,
}

impl Reply {
    pub fn file(body: impl Into<Vec<u8>>) -> Self {
        Self { status: "200 OK", headers: Vec::new(), body: body.into(), claimed_bytes: None }
    }

    /// The file compressed in transit, which the client decompresses on its own.
    pub fn gzipped(body: &[u8]) -> io::Result<Self> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(body)?;
        Ok(Self { headers: vec!["Content-Encoding: gzip".to_string()], ..Self::file(encoder.finish()?) })
    }

    pub fn redirect(to: &str) -> Self {
        Self { status: "302 Found", headers: vec![format!("Location: {to}")], ..Self::file(Vec::new()) }
    }

    /// Part of a file, as an answer to a request for a range of it.
    pub fn partial(body: impl Into<Vec<u8>>) -> Self {
        Self { status: "206 Partial Content", ..Self::file(body) }
    }

    /// A file that says it's `claimed_bytes` long, and is cut off after `body`, as a dropped connection would be.
    pub fn cut_off(body: impl Into<Vec<u8>>, claimed_bytes: usize) -> Self {
        Self { claimed_bytes: Some(claimed_bytes), ..Self::file(body) }
    }
}

pub struct Server {
    pub base: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Server {
    /// The paths asked for, in order.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().clone()
    }
}

/// Serves each path's replies in turn, then its last one from then on, and a 404 for any other path, until the test
/// process ends. A connection it can't read from is left unanswered.
pub fn serve(paths: Vec<(String, Vec<Reply>)>) -> io::Result<Server> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&asked);
    std::thread::spawn(move || {
        let mut answered = vec![0; paths.len()];
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 4096];
            let Ok(read) = stream.read(&mut request) else { continue };
            let request = String::from_utf8_lossy(&request[..read]).to_string();
            let target = request.split_whitespace().nth(1).unwrap_or_default();
            let path = target.trim_start_matches('/').split('?').next().unwrap_or_default().to_string();
            let reply = match paths.iter().position(|(served, _)| *served == path) {
                Some(at) => {
                    let replies = &paths[at].1;
                    answered[at] += 1;
                    replies[(answered[at] - 1).min(replies.len() - 1)].clone()
                }
                None => Reply { status: "404 Not Found", ..Reply::file(Vec::new()) },
            };
            log.lock().push(path);
            // The client may hang up as soon as it has read enough.
            let _ = answer(&mut stream, &reply);
        }
    });
    Ok(Server { base, asked })
}

fn answer(stream: &mut impl Write, reply: &Reply) -> io::Result<()> {
    let length = reply.claimed_bytes.unwrap_or(reply.body.len());
    write!(stream, "HTTP/1.1 {}\r\nContent-Length: {length}\r\nConnection: close\r\n", reply.status)?;
    for header in &reply.headers {
        write!(stream, "{header}\r\n")?;
    }
    stream.write_all(b"\r\n")?;
    stream.write_all(&reply.body)
}
