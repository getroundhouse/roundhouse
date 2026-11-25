//! What a browser does with the listener, for tests: fetch one of its pages.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

use std::io::{Read as _, Write as _};
use std::net::TcpStream;

/// What a page answered with.
#[derive(Debug)]
pub struct Fetched {
    pub status: u16,
    pub body: String,
}

/// What `url`, an `http` address on `127.0.0.1`, answers a `GET` with, read whole, as a browser asks.
pub fn fetch(url: &str) -> Fetched {
    let (address, target) = url.strip_prefix("http://").unwrap().split_once('/').unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    let asked = format!("GET /{target} HTTP/1.1\r\nHost: {address}\r\nAccept: text/html\r\nConnection: close\r\n\r\n");
    stream.write_all(asked.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Fetched { status, body: body.to_string() }
}
