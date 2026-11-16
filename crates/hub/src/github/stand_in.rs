//! A stand-in for GitHub on 127.0.0.1, for the hub's tests and tests of a real hub, serving both its API and its device
//! flow at one address. It answers each token's `GET /user`, each ask for a sign-in's code, each ask after a device
//! code's token, and each renewal of a sign-in's token, as the test says, when the test says, and keeps what it was
//! asked.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

/// What the stand-in answers a call with.
#[derive(Clone)]
pub enum Answer {
    /// This status, these headers and this body.
    With { status: u16, headers: Vec<(&'static str, &'static str)>, body: String },
    /// Nothing at all: it takes the request and holds the connection open.
    Never,
    /// What the boxed answer says, once the gate has been opened.
    After(Gate, Box<Answer>),
}

/// What holds an answer back until a test opens it, as [`Answer::After`] says.
#[derive(Clone)]
pub struct Gate(Arc<watch::Sender<bool>>);

impl Gate {
    /// One no answer has passed yet.
    pub fn closed() -> Self {
        Gate(Arc::new(watch::Sender::new(false)))
    }

    /// Lets every answer it holds back go, and every one after.
    pub fn open(&self) {
        self.0.send_replace(true);
    }

    async fn opened(&self) {
        self.0.subscribe().wait_for(|open| *open).await.unwrap();
    }
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

    /// A sign-in's code, `device` to ask with, for the person to type as `WDJB-MJHT`, lasting `expires_in` seconds with
    /// `interval` seconds between asks, as `POST /login/device/code` gives one.
    pub fn code(device: &str, expires_in: u64, interval: u64) -> Self {
        let body = serde_json::json!({
            "device_code": device,
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://github.com/login/device",
            "expires_in": expires_in,
            "interval": interval,
        });
        Self::json(&body)
    }

    /// The token `token`, which lasts 8 hours, and `refresh`, which lasts about 181 days, as GitHub grants a sign-in.
    pub fn granted(token: &str, refresh: &str) -> Self {
        let body = serde_json::json!({
            "access_token": token,
            "token_type": "bearer",
            "scope": "gist,project,read:org,repo,user:email,workflow,write:packages",
            "expires_in": 28_800,
            "refresh_token": refresh,
            "refresh_token_expires_in": 15_638_400,
        });
        Self::json(&body)
    }

    /// The error `error` of GitHub's device flow, such as `authorization_pending`, which it answers with a 200.
    pub fn device_flow_error(error: &str) -> Self {
        let body = serde_json::json!({"error": error, "error_description": "Made up.", "error_uri": "https://docs"});
        Self::json(&body)
    }

    /// GitHub saying it's asked too often, with the interval to keep to from then on, if it says one.
    pub fn slow_down(interval: Option<u64>) -> Self {
        let mut body = serde_json::json!({"error": "slow_down", "error_description": "Too many requests."});
        if let Some(interval) = interval {
            body["interval"] = interval.into();
        }
        Self::json(&body)
    }

    fn json(body: &serde_json::Value) -> Self {
        Answer::With { status: 200, headers: Vec::new(), body: body.to_string() }
    }
}

/// What the stand-in answers each call with. A call it has run out of answers for gets the last again.
#[derive(Default)]
pub struct Script {
    /// `GET /user`, by the token asked with. Any other is a 401, as GitHub answers a token it doesn't know.
    pub users: Vec<(&'static str, Answer)>,
    /// `POST /login/device/code`, in turn.
    pub codes: Vec<Answer>,
    /// `POST /login/oauth/access_token`, in turn, by the device code asked with. Any other is GitHub's
    /// `incorrect_device_code`.
    pub polls: Vec<(&'static str, Vec<Answer>)>,
    /// `POST /login/oauth/access_token` renewing a sign-in's token, in turn, by what renews it asked with. Any other is
    /// GitHub's `bad_refresh_token`, as it answers one it doesn't know, or that has ended, been revoked or been used.
    pub renewals: Vec<(&'static str, Vec<Answer>)>,
}

/// What it answers with still, as it goes through the script.
struct Answering {
    users: HashMap<&'static str, Answer>,
    codes: VecDeque<Answer>,
    polls: HashMap<String, VecDeque<Answer>>,
    renewals: HashMap<String, VecDeque<Answer>>,
}

impl Answering {
    fn next(answers: &mut VecDeque<Answer>) -> Option<Answer> {
        if answers.len() > 1 { answers.pop_front() } else { answers.front().cloned() }
    }
}

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
        let answering = Arc::new(Mutex::new(Answering {
            users: script.users.into_iter().collect(),
            codes: script.codes.into(),
            polls: script.polls.into_iter().map(|(device, answers)| (device.to_string(), answers.into())).collect(),
            renewals: script
                .renewals
                .into_iter()
                .map(|(refresh, answers)| (refresh.to_string(), answers.into()))
                .collect(),
        }));
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

/// What the script has for the request whose head is `head`, and whose body is `body`.
fn chosen(answering: &mut Answering, head: &str, body: &str) -> Answer {
    if head.starts_with("POST /login/device/code ") {
        return Answering::next(&mut answering.codes).unwrap_or_else(|| Answer::status(404, Vec::new()));
    }
    if head.starts_with("POST /login/oauth/access_token ") {
        let asked: serde_json::Value = serde_json::from_str(body).unwrap();
        if let Some(refresh) = asked["refresh_token"].as_str() {
            let renewals = answering.renewals.get_mut(refresh);
            return renewals
                .and_then(Answering::next)
                .unwrap_or_else(|| Answer::device_flow_error("bad_refresh_token"));
        }
        let device = asked["device_code"].as_str().unwrap_or_default();
        let polls = answering.polls.get_mut(device);
        return polls.and_then(Answering::next).unwrap_or_else(|| Answer::device_flow_error("incorrect_device_code"));
    }
    let token = head.lines().find_map(|line| line.strip_prefix("authorization: Bearer ")).unwrap_or_default();
    answering.users.get(token).cloned().unwrap_or_else(|| Answer::status(401, Vec::new()))
}
