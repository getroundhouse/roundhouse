//! What a browser and GitHub do with the listener, for tests: fetch one of its pages, read the form the first posts to
//! GitHub, and send the person back as GitHub would once they've made the app, with a code and the form's `state`.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

use std::io::{Read as _, Write as _};
use std::net::TcpStream;

/// What a page answered with.
#[derive(Debug)]
pub struct Fetched {
    pub status: u16,
    pub body: String,
}

/// The form the listener's page posts to GitHub.
#[derive(Debug)]
pub struct Form {
    /// Where it's posted, with the `state` after it.
    pub action: String,
    pub manifest: serde_json::Value,
}

impl Form {
    /// The `state` it sends GitHub.
    pub fn state(&self) -> &str {
        self.action.split_once("?state=").unwrap().1
    }
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

/// The form `page` posts, as a browser reads it.
pub fn form(page: &str) -> Form {
    let quoted = |after: &str| unescaped(page.split_once(after).unwrap().1.split_once('"').unwrap().0);
    let action = quoted("action=\"");
    let manifest = serde_json::from_str(&quoted("name=\"manifest\" value=\"")).unwrap();
    Form { action, manifest }
}

/// What GitHub does once the person has made the app from the form the page at `page_url` posts: sends the browser
/// back where the manifest says, with `code` and the form's `state`. Answers with what the listener said to that.
pub fn come_back(page_url: &str, code: &str) -> Fetched {
    let form = form(&fetch(page_url).body);
    let back = form.manifest["redirect_url"].as_str().unwrap();
    fetch(&format!("{back}?code={code}&state={}", form.state()))
}

fn unescaped(text: &str) -> String {
    let replaced = [("&quot;", "\""), ("&lt;", "<"), ("&gt;", ">"), ("&#39;", "'"), ("&amp;", "&")];
    replaced.into_iter().fold(text.to_string(), |text, (escaped, was)| text.replace(escaped, was))
}
