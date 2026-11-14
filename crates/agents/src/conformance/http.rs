//! A loopback model server shared by the API-specific scripted models. Request headers are never captured.

use std::convert::Infallible;
use std::panic::AssertUnwindSafe;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use http_body_util::{BodyExt, Channel, Full, combinators::BoxBody};
use hyper::{
    Method, StatusCode,
    body::{Bytes, Incoming},
    header::HeaderValue,
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::{net::TcpListener, task::JoinHandle};

pub(crate) type Body = BoxBody<Bytes, Infallible>;
type Script = dyn Fn(&str, &Value, u64) -> hyper::Response<Body> + Send + Sync;

pub(crate) struct Server {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl Server {
    pub(crate) async fn start(
        script: impl Fn(&str, &Value, u64) -> hyper::Response<Body> + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let held = requests.clone();
        let script: Arc<Script> = Arc::new(script);
        let next = Arc::new(AtomicU64::new(0));
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (script, requests, next) = (script.clone(), held.clone(), next.clone());
                tokio::spawn(async move {
                    let service =
                        service_fn(move |request| answer(script.clone(), requests.clone(), next.clone(), request));
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
                });
            }
        });
        Ok(Self { url, requests, task })
    }
    pub(crate) fn url(&self) -> &str {
        &self.url
    }
    pub(crate) fn port(&self) -> &str {
        self.url.rsplit(':').next().unwrap_or_default()
    }
    pub(crate) fn requests(&self) -> Vec<Value> {
        self.requests.lock().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn answer(
    script: Arc<Script>,
    requests: Arc<Mutex<Vec<Value>>>,
    next: Arc<AtomicU64>,
    request: hyper::Request<Incoming>,
) -> Result<hyper::Response<Body>, Infallible> {
    let (parts, body) = request.into_parts();
    let body = body.collect().await.map_or_default(http_body_util::Collected::to_bytes);
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    requests.lock().push(json!({"method":parts.method.as_str(),"path":parts.uri.path(),"body":body}));
    if parts.method != Method::POST {
        return Ok(json_response(StatusCode::NOT_FOUND, &json!({"error":"not scripted"})));
    }
    let made = next.fetch_add(1, Ordering::Relaxed);
    Ok(std::panic::catch_unwind(AssertUnwindSafe(|| script(parts.uri.path(), &body, made)))
        .unwrap_or_else(|_| json_response(StatusCode::INTERNAL_SERVER_ERROR, &json!({"error":"the script panicked"}))))
}

pub(crate) fn json_response(status: StatusCode, value: &Value) -> hyper::Response<Body> {
    let mut response = hyper::Response::new(Full::new(Bytes::from(value.to_string())).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert("content-type", HeaderValue::from_static("application/json"));
    response
}

pub(crate) fn events(events: Vec<(&'static str, Value)>, made: u64) -> hyper::Response<Body> {
    let (mut sender, body) = Channel::<Bytes, Infallible>::new(4);
    tokio::spawn(async move {
        for (name, data) in events {
            if sender.send_data(Bytes::from(format!("event: {name}\ndata: {data}\n\n"))).await.is_err() {
                return;
            }
        }
    });
    let mut response = hyper::Response::new(body.boxed());
    response.headers_mut().insert("content-type", HeaderValue::from_static("text/event-stream"));
    if let Ok(id) = HeaderValue::from_str(&format!("req_scripted_{made}")) {
        response.headers_mut().insert("request-id", id);
    }
    response
}
