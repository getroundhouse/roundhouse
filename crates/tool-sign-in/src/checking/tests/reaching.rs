//! Reaching a server, where its key goes, and how long and how much it's given.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use fetching::serving::NOWHERE;
use http::header::LOCATION;
use http_body_util::{Either, Full};
use protocol::client::tool_server_checks::{NoToolList, ToolServerUnreached};
use rustls::ServerConfig;
use rustls::pki_types::PrivatePkcs8KeyDer;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};
use tokio_rustls::TlsAcceptor;

use super::stand_in::{list_tools, url};
use super::{Heard, KEY, Reply, StandIn, as_json, held_after, listed, status};
use crate::ToolSignIn;
use crate::checking::{CHECK_WITHIN, LONGEST_ANSWER};
use crate::fetch::Names;

fn unreached(why: ToolServerUnreached) -> Result<u32, NoToolList> {
    Err(NoToolList::Unreached { why: Some(why) })
}

fn moved(code: u16, to: &str) -> Reply {
    let mut reply = status(code);
    reply.headers_mut().insert(LOCATION, to.parse().unwrap());
    reply
}

/// A server that answers at `/moved`, and anywhere else says it's at `to`.
async fn moving(code: u16, to: impl Fn(u16) -> String + Send + Sync + 'static) -> StandIn {
    StandIn::answering(move |heard: &Heard| {
        if heard.path == "/moved" {
            return as_json(&listed());
        }
        let here = heard.header("host").unwrap().rsplit(':').next().unwrap();
        moved(code, &to(here.parse().unwrap()))
    })
    .await
}

#[tokio::test]
async fn a_redirect_is_followed_within_its_origin_and_the_key_goes_to_no_other() {
    let elsewhere = StandIn::answering(|_| as_json(&listed())).await;
    let other = elsewhere.port;

    for code in [307, 308] {
        let within = moving(code, |_| "/moved".into()).await;
        assert_eq!(within.with_key().await, Ok(2));
        assert!(within.heard().iter().all(|heard| heard.header("authorization") == Some(KEY)));
        let whole = moving(code, |here| format!("http://127.0.0.1:{here}/moved")).await;
        assert_eq!(whole.with_key().await, Ok(2));
        let away = moving(code, move |_| format!("http://127.0.0.1:{other}/moved")).await;
        assert_eq!(away.with_key().await, Err(NoToolList::Redirected));
        let named = moving(code, |here| format!("http://localhost:{here}/moved")).await;
        assert_eq!(named.with_key().await, Err(NoToolList::Redirected));
    }
    for code in [301, 302, 303] {
        let moved = moving(code, |_| "/moved".into()).await;
        assert_eq!(moved.with_key().await, Err(NoToolList::Redirected), "{code}");
    }
    let circles = moving(307, |_| "/mcp".into()).await;
    assert_eq!(circles.with_key().await, Err(NoToolList::Redirected));
    assert!(elsewhere.heard().is_empty(), "{:?}", elsewhere.heard());
}

#[tokio::test]
async fn a_server_there_is_no_connecting_to_says_why() {
    let nowhere = NOWHERE.replace("https://", "http://");
    assert_eq!(list_tools(&url(&nowhere), true).await, unreached(ToolServerUnreached::ConnectionRefused));
    let nameless = ToolSignIn { names: Names::Fixed(HashMap::new()), ..ToolSignIn::new("https://client.example") };
    let nameless = nameless.list_tools(&url("http://mcp.example.com/mcp"), None).await;
    assert_eq!(nameless, unreached(ToolServerUnreached::NoSuchName));

    let untrusted = self_signed().await;
    assert_eq!(list_tools(&url(&untrusted), true).await, unreached(ToolServerUnreached::NotTrusted));
    let plain = StandIn::answering(|_| as_json(&listed())).await;
    let over_tls = list_tools(&url(&format!("https://127.0.0.1:{}/mcp", plain.port)), true).await;
    assert_eq!(over_tls, unreached(ToolServerUnreached::NotHttps));
    assert!(plain.heard().is_empty(), "and it's sent no key");
}

/// The address of a server whose certificate it signed itself.
async fn self_signed() -> String {
    let key = rcgen::KeyPair::generate().unwrap();
    let certificate = rcgen::CertificateParams::new(vec!["127.0.0.1".into()]).unwrap().self_signed(&key).unwrap();
    let config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.der().clone()], PrivatePkcs8KeyDer::from(key.serialize_der()).into())
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = format!("https://{}/mcp", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            // Which the client breaks off.
            let _broken_off = TlsAcceptor::from(Arc::new(config.clone())).accept(stream).await;
        }
    });
    at
}

#[tokio::test]
async fn a_server_that_goes_before_it_answers_is_lost() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = url(&format!("http://{}/mcp", listener.local_addr().unwrap()));
    tokio::spawn(async move {
        loop {
            drop(listener.accept().await.unwrap());
        }
    });
    let ends = StandIn::answering(|_| {
        let mut reply = status(200);
        reply.headers_mut().insert("content-type", "text/event-stream".parse().unwrap());
        *reply.body_mut() = Either::Left(Full::from(": nothing\n\n"));
        reply
    })
    .await;

    assert_eq!(list_tools(&at, true).await, Err(NoToolList::Lost));
    assert_eq!(ends.with_key().await, Err(NoToolList::Lost), "a stream that ends with no answer");
}

#[tokio::test]
async fn no_more_of_an_answer_is_read_than_a_limit() {
    let long = || vec![b' '; LONGEST_ANSWER + 1];

    for content_type in ["application/json", "text/event-stream"] {
        let server = StandIn::answering(move |_| held_after(content_type, long())).await;
        assert_eq!(server.with_key().await, Err(NoToolList::TooLong), "{content_type}");
    }
    let mut fits = listed().to_string().into_bytes();
    fits.resize(LONGEST_ANSWER, b' ');
    let fits = StandIn::answering(move |_| {
        let mut reply = as_json(&listed());
        *reply.body_mut() = Either::Left(Full::from(fits.clone()));
        reply
    })
    .await;
    assert_eq!(fits.with_key().await, Ok(2));
}

/// A server that hangs at `hangs`, having sent its headers, and gives a session before its tools are asked for, and
/// that, being asked for them, with how it went once it was asked that.
async fn hanging(hangs: &'static str) -> (StandIn, JoinHandle<Result<u32, NoToolList>>) {
    let server = StandIn::answering(move |heard: &Heard| match heard.asks() {
        asks if asks == hangs => held_after("application/json", "{"),
        "initialize" => {
            let mut reply = as_json(&listed());
            reply.headers_mut().insert("mcp-session-id", "made-up-session".parse().unwrap());
            reply
        }
        _ => status(400),
    })
    .await;
    let at = server.url();
    let asking = tokio::spawn(async move { list_tools(&at, true).await });
    let asked = || server.asked().last().is_some_and(|asked| asked == hangs);
    waiting::task::until(Duration::from_secs(10), "it wasn't asked", asked).await;
    (server, asking)
}

/// Time stands still once the server has been asked, and moves only by tokio's paused clock, so nothing is waited out.
#[tokio::test]
async fn a_server_that_doesnt_answer_in_time_is_given_up_on_within_the_limit() {
    for hangs in ["server/discover", "tools/list"] {
        let (_server, asking) = hanging(hangs).await;

        tokio::time::pause();
        let from = Instant::now();
        let gave_up = asking.await.unwrap();

        assert_eq!(gave_up, Err(NoToolList::TimedOut), "{hangs}");
        assert!(from.elapsed() <= CHECK_WITHIN, "{hangs}: {:?}", from.elapsed());
        tokio::time::resume();
    }
}

/// The clock is moved once, to when it's given up on, since the request that ends the session races one that stands
/// still.
#[tokio::test]
async fn a_session_is_ended_once_its_server_is_given_up_on() {
    let (server, asking) = hanging("tools/list").await;

    tokio::time::pause();
    tokio::time::advance(CHECK_WITHIN).await;
    tokio::time::resume();

    let gave_up = timeout(Duration::from_secs(10), asking).await.expect("it wasn't given up on").unwrap();
    assert_eq!(gave_up, Err(NoToolList::TimedOut));
    assert_eq!(server.asked().last().unwrap(), "DELETE");
}
