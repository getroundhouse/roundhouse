//! A web server on this Mac for the tests, standing in for where updates are published. It answers each path as it's
//! told, and keeps the paths it was asked for.

use std::io::{Read as _, Write};
use std::net::TcpListener;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use parking_lot::Mutex;

/// One answer to a request.
#[derive(Clone)]
pub(crate) struct Reply {
    status: &'static str,
    headers: Vec<String>,
    body: Vec<u8>,
}

impl Reply {
    pub(crate) fn file(body: impl Into<Vec<u8>>) -> Self {
        Self { status: "200 OK", headers: Vec::new(), body: body.into() }
    }

    /// The file compressed in transit, which the client decompresses on its own.
    pub(crate) fn gzipped(body: &[u8]) -> Self {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(body).unwrap();
        Self { status: "200 OK", headers: vec!["Content-Encoding: gzip".to_string()], body: encoder.finish().unwrap() }
    }

    pub(crate) fn redirect(to: &str) -> Self {
        Self { status: "302 Found", headers: vec![format!("Location: {to}")], body: Vec::new() }
    }
}

pub(crate) struct Server {
    pub(crate) base: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Server {
    /// The paths asked for, in order.
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked.lock().clone()
    }
}

/// Serves each path's replies in turn, then its last one from then on, and a 404 for any other path, until the test
/// process ends.
pub(crate) fn serve(paths: Vec<(String, Vec<Reply>)>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&asked);
    std::thread::spawn(move || {
        let mut answered = vec![0; paths.len()];
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
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
                None => Reply { status: "404 Not Found", headers: Vec::new(), body: Vec::new() },
            };
            log.lock().push(path);
            // The client may hang up as soon as it has read enough.
            let _ = answer(&mut stream, &reply);
        }
    });
    Server { base, asked }
}

fn answer(stream: &mut impl Write, reply: &Reply) -> std::io::Result<()> {
    write!(stream, "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n", reply.status, reply.body.len())?;
    for header in &reply.headers {
        write!(stream, "{header}\r\n")?;
    }
    stream.write_all(b"\r\n")?;
    stream.write_all(&reply.body)
}
