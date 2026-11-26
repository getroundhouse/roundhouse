//! The listener alone, on real sockets on `127.0.0.1`, asked as a browser and GitHub would ask it.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use protocol::client::github_apps::GithubManifestCode;
use serde_json::json;

use super::browsing::{come_back, form};
use super::listening::Listening;
use super::page;
use super::{Bound, Ended, Organization, Under};
use crate::loopback::browsing::fetch;
use crate::loopback::page::said;

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
    waiting::thread::until(SOON, format!("still listening at {address}"), || TcpStream::connect(address).is_err());
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
    let ended = listening.ended();

    let back = come_back(listening.page_url(), "a-made-up-code");

    assert_eq!((back.status, back.body), (200, said(page::BACK)));
    assert_eq!(ended.recv_timeout(SOON).unwrap(), Ended::Heard(GithubManifestCode::new("a-made-up-code")));
    until_refused(address);
}

#[test]
fn it_refuses_another_state_or_none_and_goes_on_listening() {
    let (listening, _, address) = serving(&Under::Account, NEVER);
    let ended = listening.ended();
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
    assert!(ended.try_recv().is_err(), "it took none of them");
    assert_eq!(come_back(listening.page_url(), "a-made-up-code").status, 200, "and still takes the right one");
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
