use std::sync::Arc;

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use super::script::{Answering, chosen};
use super::{Answer, Script};

/// The stand-in, serving until the runtime it was started on ends.
pub struct StandIn {
    api: String,
    /// The head of each request it was sent, lowercased, and then its body, in the order they came.
    asked: Arc<Mutex<Vec<String>>>,
    asked_count: watch::Receiver<usize>,
}

impl StandIn {
    /// Serves on a port of its own, answering each token's `GET /user` in `users` as it says.
    pub async fn start(users: Vec<(&'static str, Answer)>) -> Self {
        Self::with(Script { users, ..Script::default() }).await
    }

    /// Serves on a port of its own, answering as `script` says.
    pub async fn with(script: Script) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let answering = Arc::new(Mutex::new(Answering::of(script)));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (count, asked_count) = watch::channel(0);
        let count = Arc::new(count);
        let kept = asked.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                tokio::spawn(answer(stream, answering.clone(), kept.clone(), count.clone()));
            }
        });
        StandIn { api, asked, asked_count }
    }

    /// Its address, as the hub's `GithubConfig` takes it for both GitHub's API and its device flow.
    pub fn api(&self) -> String {
        self.api.clone()
    }

    /// Each request it was sent: its request line and headers, lowercased, and then its body as it came.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().clone()
    }

    /// Once it has been sent `count` requests.
    pub async fn until_asked(&self, count: usize) {
        self.asked_count.clone().wait_for(|asked| *asked >= count).await.unwrap();
    }
}

/// Reads one request from `stream`, and answers as `answering` says for it.
async fn answer(
    mut stream: TcpStream,
    answering: Arc<Mutex<Answering>>,
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
    let lowercase = head.to_lowercase();
    let length =
        lowercase.lines().find_map(|line| line.strip_prefix("content-length: ")).map_or(0, |n| n.parse().unwrap());
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    let body = String::from_utf8(body).unwrap();
    let mut answer = chosen(&mut answering.lock(), &head, &body);
    asked.lock().push(format!("{lowercase}{body}"));
    count.send_modify(|count| *count += 1);
    while let Answer::After(gate, then) = answer {
        gate.opened().await;
        answer = *then;
    }
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
