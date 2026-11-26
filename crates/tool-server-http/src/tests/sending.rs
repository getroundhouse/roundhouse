use std::error::Error;
use std::sync::Arc;

use http::header::{HeaderMap, HeaderValue};
use http::{Method, Request, Response};
use http_body_util::{BodyExt as _, Full};
use hyper::body::{Bytes, Incoming};
use parking_lot::Mutex;
use protocol::model::{ToolServerHeader, ToolServerUrl};
use protocol::worker::{ServerHeader, ServerHeaderValue};

use crate::{KEY, Names, Redirected, Server, StandIn, Tls, send};

const LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

fn reply(status: u16, location: Option<&str>, body: &'static str) -> Response<Full<Bytes>> {
    let mut reply = Response::builder().status(status);
    if let Some(location) = location {
        reply = reply.header("location", location);
    }
    reply.body(Full::from(body)).unwrap()
}

/// Sends `server` a message, with `headers`, as the hub's check does, and gives what came of it, with its body.
async fn sent(server: &Server, headers: &[(&'static str, &str)]) -> Result<String, Redirected> {
    let headers: HeaderMap =
        headers.iter().map(|(name, value)| (name.parse().unwrap(), HeaderValue::from_str(value).unwrap())).collect();
    let tls = Tls::default();
    let sending = async |request: Request<Full<Bytes>>| {
        let addresses = Names::System.at(request.uri()).await?;
        send(&tls, addresses, request).await
    };
    let answered: Result<_, Box<dyn Error + Send + Sync>> =
        server.send(&Method::POST, &headers, &Bytes::from(LIST), sending).await;
    let answer: Response<Incoming> = answered.unwrap()?;
    Ok(String::from_utf8(answer.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

/// A server with no header is sent none, and one's own header takes the place of any of its name it would be sent.
#[tokio::test]
async fn a_servers_own_header_goes_with_each_request_over_any_of_its_name() {
    let stand_in = StandIn::answering(|_| reply(200, None, "")).await;
    let own = ServerHeader { name: ToolServerHeader::typed("Mcp-Key").unwrap(), ..StandIn::key() };

    for header in [Some(own), None] {
        let server = Server::new(&stand_in.url(), header.as_ref()).unwrap();
        sent(&server, &[("mcp-key", "another"), ("accept", "application/json")]).await.unwrap();
    }

    let heard = stand_in.heard();
    assert_eq!(heard[0].headers.get_all("mcp-key").iter().collect::<Vec<_>>(), [KEY]);
    assert_eq!(heard[1].header("mcp-key"), Some("another"));
    for asked in &heard {
        assert_eq!(
            (&asked.method, asked.path.as_str(), &asked.body[..]),
            (&Method::POST, "/mcp?query=made-up-query", LIST.as_bytes())
        );
        assert_eq!(asked.header("accept"), Some("application/json"));
    }
}

#[test]
fn a_url_or_a_header_http_cant_carry_makes_no_server() {
    let header = |name: &str, value: &str| ServerHeader {
        name: ToolServerHeader::typed(name).unwrap(),
        value: ServerHeaderValue::new(value),
    };
    let url = ToolServerUrl::typed("https://tools.example/mcp").unwrap();

    assert!(Server::new(&url, Some(&header("Authorization", "Bearer a"))).is_some_and(|server| server.keyed()));
    assert!(Server::new(&url, None).is_some_and(|server| !server.keyed()));
    assert!(Server::new(&url, Some(&header("Authorization", "a\nb"))).is_none());
    for stored in ["ftp://tools.example/mcp", "tools.example/mcp", "https:///mcp"] {
        assert!(Server::new(&ToolServerUrl::stored(stored.into()), None).is_none(), "{stored}");
    }
}

/// Only within the origin the key is for, and only one that keeps the method and the body.
#[tokio::test]
async fn a_redirect_is_followed_only_within_its_origin() {
    let elsewhere = StandIn::answering(|_| reply(200, None, "elsewhere")).await;
    let other = elsewhere.port;
    let to = Arc::new(Mutex::new((307, String::new())));
    let told = to.clone();
    let stand_in = StandIn::answering(move |asked| {
        let (status, location) = told.lock().clone();
        if asked.path == "/moved" { reply(200, None, "moved") } else { reply(status, Some(&location), "") }
    })
    .await;
    let server = Server::new(&stand_in.url(), Some(&StandIn::key())).unwrap();
    let here = stand_in.port;
    let followed = [(307, "/moved".into()), (308, "/moved".into()), (308, format!("http://127.0.0.1:{here}/moved"))];
    let not_followed = [
        (307, format!("http://127.0.0.1:{other}/moved")),
        (308, format!("http://localhost:{here}/moved")),
        (307, format!("https://127.0.0.1:{here}/moved")),
        (307, format!("//127.0.0.1:{other}/moved")),
        (301, "/moved".into()),
        (302, "/moved".into()),
        (303, "/moved".into()),
        (307, "moved".into()),
        // In circles.
        (307, "/mcp".into()),
    ];

    for redirect in followed {
        *to.lock() = redirect.clone();
        assert_eq!(sent(&server, &[]).await.as_deref(), Ok("moved"), "{redirect:?}");
        let last = stand_in.heard().pop().unwrap();
        let sent = (&last.method, &last.body[..], last.header("authorization"));
        assert_eq!(sent, (&Method::POST, LIST.as_bytes(), Some(KEY)), "{redirect:?}");
    }
    for redirect in not_followed {
        *to.lock() = redirect.clone();
        assert_eq!(sent(&server, &[]).await, Err(Redirected), "{redirect:?}");
    }
    assert!(elsewhere.heard().is_empty(), "the key left its origin: {:?}", elsewhere.heard());
}
