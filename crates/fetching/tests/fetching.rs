//! Fetching from a local server that answers as each test tells it.

#![cfg(test)]

use std::time::Duration;

use fetching::serving::{Reply, serve};
use fetching::{FetchError, Fetcher};

fn fetcher() -> Fetcher {
    Fetcher::new(Duration::from_secs(10))
}

#[test]
fn a_body_comes_decompressed() {
    let server = serve(vec![("feed".to_string(), vec![Reply::gzipped(b"the feed").unwrap()])]).unwrap();

    assert_eq!(fetcher().bytes(&format!("{}/feed", server.base), 100).unwrap(), b"the feed");
}

#[test]
fn a_body_longer_than_the_limit_once_decompressed_is_refused() {
    let server = serve(vec![("feed".to_string(), vec![Reply::gzipped(&vec![b' '; 1001]).unwrap()])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 1000);

    assert!(matches!(fetched, Err(FetchError::TooLong { limit_bytes: 1000, .. })), "{fetched:?}");
}

/// Were the rest read, the connection's end would be what stopped it.
#[test]
fn a_body_past_the_limit_is_refused_without_reading_the_rest() {
    let server = serve(vec![("feed".to_string(), vec![Reply::cut_off(vec![b' '; 1001], 1_000_000)])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 1000);

    assert!(matches!(fetched, Err(FetchError::TooLong { limit_bytes: 1000, .. })), "{fetched:?}");
}

#[test]
fn a_body_cut_off_in_transit_cant_be_read() {
    let server = serve(vec![("feed".to_string(), vec![Reply::cut_off("12345", 10)])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 100);

    assert!(matches!(fetched, Err(FetchError::Read { .. })), "{fetched:?}");
}

#[test]
fn an_answer_other_than_the_whole_file_is_refused() {
    let server = serve(vec![("feed".to_string(), vec![Reply::partial("the feed")])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 100);

    assert!(matches!(fetched, Err(FetchError::Status { status: 206, .. })), "{fetched:?}");
}

#[test]
fn a_body_as_long_as_the_limit_is_taken() {
    let server = serve(vec![("feed".to_string(), vec![Reply::file(vec![b' '; 1000])])]).unwrap();

    assert_eq!(fetcher().bytes(&format!("{}/feed", server.base), 1000).unwrap().len(), 1000);
}

#[test]
fn a_redirect_is_an_answer_and_not_followed() {
    let elsewhere = serve(vec![("feed".to_string(), vec![Reply::file("the feed")])]).unwrap();
    let server = serve(vec![("feed".to_string(), vec![Reply::redirect(&format!("{}/feed", elsewhere.base))])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 100);

    assert!(matches!(fetched, Err(FetchError::Status { status: 302, .. })), "{fetched:?}");
    assert!(elsewhere.asked().is_empty());
}

#[test]
fn a_file_that_isnt_there_says_so_and_names_its_address() {
    let server = serve(vec![]).unwrap();
    let url = format!("{}/feed", server.base);

    let fetched = fetcher().bytes(&url, 100);

    assert!(matches!(&fetched, Err(FetchError::Status { url: named, status: 404 }) if *named == url), "{fetched:?}");
}

#[test]
fn a_server_that_isnt_there_says_so_and_names_its_address() {
    let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let url = format!("http://{gone}/feed");

    let fetched = fetcher().bytes(&url, 100);

    assert!(matches!(&fetched, Err(FetchError::Request { url: named, .. }) if *named == url), "{fetched:?}");
}

#[test]
fn a_file_is_written_whole() {
    let server = serve(vec![("package".to_string(), vec![Reply::gzipped(b"12345").unwrap()])]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let to = dir.path().join("package");

    fetcher().to_file(&format!("{}/package", server.base), &to, 5).unwrap();

    assert_eq!(std::fs::read(to).unwrap(), b"12345");
}

#[test]
fn a_file_shorter_or_longer_than_it_should_be_is_refused() {
    let server = serve(vec![
        ("short".to_string(), vec![Reply::file("12345")]),
        ("long".to_string(), vec![Reply::file("123456")]),
    ])
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let to = dir.path().join("package");

    let short = fetcher().to_file(&format!("{}/short", server.base), &to, 6);
    let long = fetcher().to_file(&format!("{}/long", server.base), &to, 5);

    assert!(matches!(short, Err(FetchError::CutShort { expected_bytes: 6, got_bytes: 5, .. })), "{short:?}");
    assert!(matches!(long, Err(FetchError::TooLong { limit_bytes: 5, .. })), "{long:?}");
}

#[test]
fn a_file_that_cant_be_saved_names_where() {
    let server = serve(vec![("package".to_string(), vec![Reply::file("12345")])]).unwrap();
    let to = tempfile::tempdir().unwrap().path().join("gone").join("package");

    let fetched = fetcher().to_file(&format!("{}/package", server.base), &to, 5);

    assert!(matches!(&fetched, Err(FetchError::Save { path, .. }) if *path == to), "{fetched:?}");
}
