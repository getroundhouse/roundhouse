use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use fetching::serving::{NOWHERE, Reply, serve};
use tokio::net::TcpListener;
use tool_server_http::{Names, Tls};
use url::Url;

use super::reach::host;
use super::{Caller, LONGEST_ANSWER, MOST_REDIRECTS, NotFetched, Reach, json};
use crate::{Failure, ToolSignIn};

/// Longer than any call may take, on tokio's paused clock.
const FOREVER: Duration = Duration::from_secs(600);

fn url(text: &str) -> Url {
    Url::parse(text).unwrap()
}

fn addresses(these: &[&str]) -> Vec<SocketAddr> {
    these.iter().map(|address| SocketAddr::new(address.parse().unwrap(), 443)).collect()
}

#[test]
fn a_server_on_the_internet_is_connected_to_there_alone_and_one_on_a_private_network_there() {
    let (on_internet, on_private) = (Reach::made_up("tools.example", false), Reach::made_up("tools.example", true));
    let (public, private) = ("1.1.1.1", "10.0.0.1");
    let none: &[&str] = &[];

    let cases = [
        // Of a server on the internet, whatever its host or another's comes to.
        (&on_internet, "https://tools.example/mcp", vec![public], vec![public]),
        (&on_internet, "https://sign-in.example", vec![private, public], vec![public]),
        (&on_internet, "https://sign-in.example", vec![private], none.to_vec()),
        (&on_internet, "https://tools.example/mcp", vec![private], none.to_vec()),
        // Of a server on a private network, which has to be there still, and what it names, anywhere over https.
        (&on_private, "https://tools.example/mcp", vec![private], vec![private]),
        (&on_private, "https://tools.example/mcp", vec![public], none.to_vec()),
        (&on_private, "http://tools.example/mcp", vec![private], vec![private]),
        (&on_private, "https://sign-in.example", vec![private, public], vec![private, public]),
        (&on_private, "http://sign-in.example", vec![private, public], vec![private]),
        (&on_private, "http://sign-in.example", vec![public], none.to_vec()),
    ];
    for (reach, text, found, expected) in cases {
        let vetted = reach.vetted(&url(text), addresses(&found));
        let expected = if expected.is_empty() { Err(NotFetched::Address) } else { Ok(addresses(&expected)) };
        assert_eq!(vetted, expected, "{text} at {found:?}");
    }
}

#[test]
fn only_https_is_fetched_but_for_a_server_on_a_private_network_and_nothing_with_a_sign_in_in_its_address() {
    let (on_internet, on_private) = (Reach::made_up("tools.example", false), Reach::made_up("tools.example", true));

    assert_eq!(on_internet.takes(&url("https://sign-in.example/token")), Ok(()));
    assert_eq!(on_internet.takes(&url("http://sign-in.example/token")), Err(NotFetched::NotHttps));
    assert_eq!(on_private.takes(&url("http://sign-in.example/token")), Ok(()));
    for reach in [on_internet, on_private] {
        for other in ["file:///etc/passwd", "ftp://sign-in.example", "javascript:alert(1)"] {
            assert_eq!(reach.takes(&url(other)), Err(NotFetched::NotHttps), "{other}");
        }
        for other in ["https://ada@sign-in.example", "https://:pass@sign-in.example", "https://sign-in.example/#a"] {
            assert_eq!(reach.takes(&url(other)), Err(NotFetched::NotBare), "{other}");
        }
    }
}

/// A name is as good as anyone who can set it, so what it comes to is what counts.
#[tokio::test]
async fn a_server_is_on_a_private_network_when_every_address_its_host_comes_to_is() {
    let found = |these: &[&str]| these.iter().map(|address| address.parse().unwrap()).collect::<Vec<IpAddr>>();
    let names = Names::Fixed(HashMap::from([
        ("private.example", found(&["10.0.0.1", "fd12::1"])),
        ("both.example", found(&["10.0.0.1", "1.1.1.1"])),
        ("public.example", found(&["1.1.1.1"])),
    ]));

    let servers = [
        ("http://private.example/mcp", true),
        ("https://both.example/mcp", false),
        ("https://public.example/mcp", false),
        ("http://127.0.0.1:3000/mcp", true),
        ("http://2130706433/mcp", true),
        ("http://[::1]:3000/mcp", true),
        ("https://1.1.1.1/mcp", false),
    ];
    for (server, private) in servers {
        let reach = Reach::of(&Url::parse(server).unwrap(), &names).await.unwrap();
        assert_eq!(reach.takes(&url("http://sign-in.example")).is_ok(), private, "{server}");
    }
}

/// Over what it would connect with, to see that nothing's connected to, rather than only that it says it wouldn't.
#[tokio::test]
async fn a_name_that_comes_to_this_machine_is_refused_before_its_connected_to() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let names = Names::Fixed(HashMap::from([("sign-in.example", vec!["127.0.0.1".parse().unwrap()])]));
    let tls = Tls::default();
    let calls = Caller::made_up(Reach::made_up("tools.example", false), &names, &tls);

    for place in [format!("https://sign-in.example:{port}/"), format!("https://127.0.0.1:{port}/")] {
        let place = url(&place);
        let failure = calls.get(&place).await.err().unwrap();
        let Failure::NotFetched { host: refused, why } = failure else { panic!("{failure:?}") };
        assert_eq!((refused, why), (host(&place), NotFetched::Address));
        let failure = calls.post_form(&place, &[]).await.err().unwrap();
        assert!(matches!(failure, Failure::NotFetched { why: NotFetched::Address, .. }), "{failure:?}");
        let failure = calls.post_json(&place, &serde_json::json!({})).await.err().unwrap();
        assert!(matches!(failure, Failure::NotFetched { why: NotFetched::Address, .. }), "{failure:?}");
    }
    let connected = std::future::poll_fn(|cx| std::task::Poll::Ready(listener.poll_accept(cx).is_ready())).await;
    assert!(!connected, "something connected");
}

#[tokio::test]
async fn metadata_is_read_through_a_few_redirects_to_what_would_have_been_fetched_at_first() {
    let hops = |count: usize| {
        let hop = move |n: usize| (format!("{count}/{n}"), vec![Reply::redirect(&format!("/{count}/{}", n + 1))]);
        let end = (format!("{count}/{count}"), vec![Reply::file(r#"{"made": "up"}"#)]);
        (0..count).map(hop).chain([end]).collect::<Vec<_>>()
    };
    let to = |path: &str, to: &str| vec![(path.to_string(), vec![Reply::redirect(to)])];
    let elsewhere = to("elsewhere", "http://sign-in.example/metadata");
    let not_fetched = to("file", "file:///etc/passwd");
    let paths = [hops(MOST_REDIRECTS), hops(MOST_REDIRECTS + 1), elsewhere, not_fetched].concat();
    let server = serve(paths).unwrap();
    let client = ToolSignIn {
        names: Names::Fixed(HashMap::from([("sign-in.example", vec!["1.1.1.1".parse().unwrap()])])),
        ..ToolSignIn::new("https://client.example")
    };
    let calls = client.calls_for(&Url::parse(&server.base).unwrap()).await.unwrap();
    let get = async |path: &str| calls.get(&url(&format!("{}{path}", server.base))).await;

    let read = get(&format!("/{MOST_REDIRECTS}/0")).await.unwrap();
    let too_many = get(&format!("/{}/0", MOST_REDIRECTS + 1)).await.unwrap();
    let elsewhere = get("/elsewhere").await.err().unwrap();
    let file = get("/file").await.err().unwrap();

    assert_eq!(json(&read), Some(serde_json::json!({"made": "up"})));
    assert_eq!(too_many.status, 302);
    assert!(matches!(elsewhere, Failure::NotFetched { why: NotFetched::Address, .. }), "{elsewhere:?}");
    assert!(matches!(file, Failure::NotFetched { why: NotFetched::NotHttps, .. }), "{file:?}");
}

#[tokio::test]
async fn an_answer_with_too_much_in_it_isnt_read_and_one_that_never_comes_isnt_waited_for() {
    let paths = [("fits", LONGEST_ANSWER), ("too-long", LONGEST_ANSWER + 1)];
    let server = serve(paths.map(|(path, long)| (path.to_string(), vec![Reply::file(vec![b'1'; long])])).into());
    let server = server.unwrap();
    let client = ToolSignIn::new("https://client.example");
    let calls = client.calls_for(&Url::parse(&server.base).unwrap()).await.unwrap();

    assert!(calls.get(&url(&format!("{}/fits", server.base))).await.is_ok());
    let too_long = calls.get(&url(&format!("{}/too-long", server.base))).await.err().unwrap();
    let nowhere = calls.get(&url(NOWHERE)).await.err().unwrap();

    assert!(matches!(too_long, Failure::Unreadable { .. }), "{too_long:?}");
    assert!(matches!(nowhere, Failure::Unanswered { .. }), "{nowhere:?}");
}

/// Time stands still once the call has connected, and moves only by tokio's paused clock, so nothing is waited out.
#[tokio::test]
async fn a_server_that_never_answers_fails_the_call_at_its_time_limit() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let call = tokio::spawn(async move {
        let client = ToolSignIn::new("https://client.example");
        let calls = client.calls_for(&Url::parse(&server).unwrap()).await.unwrap();
        calls.get(&url(&server)).await.err().unwrap()
    });
    let _connected = listener.accept().await.unwrap();

    tokio::time::pause();
    let failure = tokio::time::timeout(FOREVER, call).await.expect("the call was still waiting").unwrap();

    assert!(matches!(failure, Failure::TimedOut { .. }), "{failure:?}");
}
