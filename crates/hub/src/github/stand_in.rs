//! A stand-in for GitHub's API on 127.0.0.1, for the hub's tests and tests of a real hub, which answers each token's
//! `GET /user` as the test says, and keeps what it was asked.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

/// What the stand-in answers a call with one token.
#[derive(Clone)]
pub enum Answer {
    /// This status, these headers and this body.
    With { status: u16, headers: Vec<(&'static str, &'static str)>, body: String },
    /// Nothing at all: it takes the request and holds the connection open.
    Never,
}

impl Answer {
    /// The user `login`, whose number is `id`, as `GET /user` gives it, with the header that says when its token ends,
    /// as GitHub writes it, such as `2027-02-11 08:00:00 UTC`.
    pub fn user(id: u64, login: &str, ends: Option<&'static str>) -> Self {
        let headers = ends.map(|ends| ("github-authentication-token-expiration", ends)).into_iter().collect();
        let body = serde_json::json!({"login": login, "id": id, "type": "User"}).to_string();
        Answer::With { status: 200, headers, body }
    }

    /// Only `status`, with a body that says nothing more, as GitHub's errors are.
    pub fn status(status: u16, headers: Vec<(&'static str, &'static str)>) -> Self {
        Answer::With { status, headers, body: r#"{"message":"made up"}"#.to_string() }
    }
}

/// The stand-in, serving until the runtime it was started on ends.
pub struct StandIn {
    api: String,
    /// The head of each request it was sent, in the order they came.
    asked: Arc<Mutex<Vec<String>>>,
    asked_count: watch::Receiver<usize>,
}

impl StandIn {
    /// Serves on a port of its own, answering each token in `answers` as it says, and any other with a 401, as GitHub
    /// does a token it doesn't know.
    pub async fn start(answers: Vec<(&'static str, Answer)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let answers: Arc<HashMap<&'static str, Answer>> = Arc::new(answers.into_iter().collect());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (count, asked_count) = watch::channel(0);
        let count = Arc::new(count);
        let kept = asked.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                tokio::spawn(answer(stream, answers.clone(), kept.clone(), count.clone()));
            }
        });
        StandIn { api, asked, asked_count }
    }

    /// Its address, as the hub's `Github` takes it.
    pub fn api(&self) -> String {
        self.api.clone()
    }

    /// The head of each request it was sent: its request line and headers, lowercased.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().clone()
    }

    /// Once it has been sent `count` requests.
    pub async fn until_asked(&self, count: usize) {
        self.asked_count.clone().wait_for(|asked| *asked >= count).await.unwrap();
    }
}

/// Reads one request's head from `stream`, and answers as `answers` says for its token.
async fn answer(
    mut stream: TcpStream,
    answers: Arc<HashMap<&'static str, Answer>>,
    asked: Arc<Mutex<Vec<String>>>,
    count: Arc<watch::Sender<usize>>,
) {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        if stream.read(&mut byte).await.unwrap() == 0 {
            return;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let token = head.lines().find_map(|line| line.strip_prefix("authorization: Bearer ")).unwrap_or_default();
    let answer = answers.get(token).cloned().unwrap_or_else(|| Answer::status(401, Vec::new()));
    asked.lock().push(head.to_lowercase());
    count.send_modify(|count| *count += 1);
    let Answer::With { status, headers, body } = answer else {
        std::future::pending::<()>().await;
        return;
    };
    let headers = headers.into_iter().map(|(name, value)| format!("{name}: {value}\r\n")).collect::<Vec<_>>().concat();
    let written = format!(
        "HTTP/1.1 {status} Made Up\r\ncontent-type: application/json\r\n{headers}content-length: {}\r\n\
         connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(written.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();
}
