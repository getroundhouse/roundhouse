//! The listener alone, on real sockets on `127.0.0.1`, asked as a browser and GitHub would ask it.

use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use protocol::client::github_apps::GithubManifestCode;
use serde_json::json;

use super::answering::Asked;
use super::browsing::{come_back, fetch, form};
use super::listening::Listening;
use super::page;
use super::{Bound, Heard, Organization, Under};

/// Longer than any test here waits for what it's waiting on.
const NEVER: Duration = Duration::from_secs(600);
/// How long a test waits for the listener to do what it should at once, at most.
const SOON: Duration = Duration::from_secs(5);

/// A manifest, as the hub writes one, with what HTML would read as markup in its description.
fn manifest(bound: &Bound) -> serde_json::Value {
    json!({"name": "ada-roundhouse", "redirect_url": bound.redirect_url(), "description": "Ada's \"own\" <app> & more"})
}

/// A listener serving [`manifest`], under `under`, until `limit`.
fn serving(under: &Under, limit: Duration) -> (Listening, serde_json::Value, SocketAddr) {
    let bound = Bound::new().unwrap();
    let manifest = manifest(&bound);
    let address = address_of(&bound.redirect_url());
    (bound.serve(&manifest, under, limit).unwrap(), manifest, address)
}

fn address_of(url: &str) -> SocketAddr {
    url.strip_prefix("http://").unwrap().split_once('/').unwrap().0.parse().unwrap()
}

/// Waits until nothing listens at `address` any more.
fn until_refused(address: SocketAddr) {
    let deadline = Instant::now() + SOON;
    while TcpStream::connect(address).is_ok() {
        assert!(Instant::now() < deadline, "still listening at {address}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn it_listens_only_on_loopback_on_a_port_the_system_picks() {
    let bound = Bound::new().unwrap();

    let address = address_of(&bound.redirect_url());

    assert_eq!(address.ip().to_string(), "127.0.0.1", "not every address this machine has");
    assert_ne!(address.port(), 0);
    assert!(bound.redirect_url().ends_with("/made"), "{}", bound.redirect_url());
}

#[test]
fn its_page_posts_the_manifest_it_was_given_to_the_form_under_the_account_or_organization_chosen() {
    let adalabs = Under::Organization(Organization::parse("adalabs").unwrap());

    for (under, form_at) in [
        (Under::Account, "https://github.com/settings/apps/new"),
        (adalabs, "https://github.com/organizations/adalabs/settings/apps/new"),
    ] {
        let (listening, manifest, _) = serving(&under, NEVER);
        let page = fetch(listening.page_url());

        assert_eq!(page.status, 200);
        let posted = form(&page.body);
        assert_eq!(posted.manifest, manifest, "as it was given, whatever it has in it");
        let state = posted.state().to_string();
        assert_eq!(posted.action, format!("{form_at}?state={state}"));
        assert_eq!(state.len(), 32, "128 bits, as hex");
        assert!(page.body.contains("document.forms[0].submit()"), "it posts itself as it loads");
    }
}

#[test]
fn it_takes_the_code_github_sends_back_with_its_state_and_then_stops() {
    let (listening, _, address) = serving(&Under::Account, NEVER);
    let heard = listening.heard();

    let back = come_back(listening.page_url(), "a-made-up-code");

    assert_eq!((back.status, back.body), (200, page::said(page::BACK)));
    assert_eq!(heard.recv_timeout(SOON).unwrap(), Heard::Code(GithubManifestCode::new("a-made-up-code")));
    until_refused(address);
}

#[test]
fn it_refuses_another_state_or_none_and_goes_on_listening() {
    let (listening, _, address) = serving(&Under::Account, NEVER);
    let heard = listening.heard();
    let at = |target: &str| format!("http://{address}{target}");

    let refused = [
        at("/made?code=a-made-up-code&state=0123456789abcdef0123456789abcdef"),
        at("/made?code=a-made-up-code"),
        at("/make?state=0123456789abcdef0123456789abcdef"),
        at("/make"),
    ];

    for url in refused {
        assert_eq!(fetch(&url).status, 400, "{url}");
    }
    assert!(heard.try_recv().is_err(), "it took none of them");
    assert_eq!(come_back(listening.page_url(), "a-made-up-code").status, 200, "and still takes the right one");
}

#[test]
fn it_refuses_any_other_path_or_method_and_goes_on_listening() {
    let (listening, _, address) = serving(&Under::Account, NEVER);
    let heard = listening.heard();
    let state = form(&fetch(listening.page_url()).body).state().to_string();

    for target in ["/", "/favicon.ico", &format!("/made/again?code=c&state={state}"), &format!("/?state={state}")] {
        assert_eq!(fetch(&format!("http://{address}{target}")).status, 404, "{target}");
    }
    let posted = Asked::of(&format!("POST /made?code=c&state={state} HTTP/1.1"));
    assert_eq!(posted, Asked::Elsewhere, "nor by another method");
    assert_eq!(Asked::of(&format!("GET /made?code=c&state={state} HTTP/2")), Asked::Elsewhere);
    assert!(heard.try_recv().is_err());
    assert_eq!(come_back(listening.page_url(), "a-made-up-code").status, 200);
}

#[test]
fn what_a_request_asks_is_read_as_a_forms_encoding_has_it() {
    let asked = Asked::of("GET /made?code=a%2Bb+c&state=s&state=another HTTP/1.1");

    let read = Asked::Made { code: Some("a+b c".to_string()), state: Some("s".to_string()) };
    assert_eq!(asked, read, "and the first of two");
}

#[test]
fn it_stops_once_its_let_go_of_having_heard_nothing() {
    let (listening, _, address) = serving(&Under::Account, NEVER);
    let heard = listening.heard();

    drop(listening);

    until_refused(address);
    assert_eq!(heard.recv_timeout(SOON), Err(flume::RecvTimeoutError::Disconnected), "it says nothing as it goes");
}

#[test]
fn it_stops_once_its_limit_has_passed_saying_so() {
    let (listening, _, address) = serving(&Under::Account, Duration::from_millis(50));
    let heard = listening.heard();

    assert_eq!(heard.recv_timeout(SOON).unwrap(), Heard::TimedOut);

    until_refused(address);
    drop(listening);
}

#[test]
fn an_organization_is_named_as_github_names_one() {
    let named = |name: &str| Organization::parse(name).map(|organization| organization.as_str().to_string());

    assert_eq!(named(" adalabs ").unwrap(), "adalabs", "without spaces around it");
    assert!(named("Ada-Labs-2").is_ok());
    assert!(named(&"a".repeat(39)).is_ok());
    for refused in ["", "-ada", "ada-", "ada--labs", "ada labs", "ada/../settings", "adä", &"a".repeat(40)] {
        assert!(named(refused).is_err(), "{refused:?}");
    }
}
