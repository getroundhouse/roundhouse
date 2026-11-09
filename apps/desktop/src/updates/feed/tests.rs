use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair as _};

use super::*;

/// The shape `vpk` writes, with one release.
const FEED: &str = r#"{"Assets":[{"PackageId":"Roundhouse","Version":"2026.1109.903","Type":"Full","FileName":"Roundhouse-2026.1109.903-osx-full.nupkg","SHA1":"AD4CF1813CB4589407AEEB7DA69063A25FF84566","SHA256":"0F568A995C5F5474207B3A3225F7B4059D2A268C35ED32310F1083E59CA286B7","Size":18889777}]}"#;

fn key_pair() -> Ed25519KeyPair {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
}

fn public(pair: &Ed25519KeyPair) -> FeedKey {
    FeedKey(pair.public_key().as_ref().try_into().unwrap())
}

/// A feed, its signature, and the key that checks it, as the release workflow makes them.
fn signed(feed: &str) -> (Vec<u8>, Vec<u8>, FeedKey) {
    let pair = key_pair();
    (feed.as_bytes().to_vec(), pair.sign(feed.as_bytes()).as_ref().to_vec(), public(&pair))
}

#[test]
fn a_feed_signed_with_the_key_is_read() {
    let (feed, signature, key) = signed(FEED);

    let releases = verified(&feed, &signature, &key).unwrap();

    assert_eq!(releases.Assets.len(), 1);
    assert_eq!(releases.Assets[0].Version, "2026.1109.903");
}

#[test]
fn a_changed_feed_is_refused() {
    let (mut feed, signature, key) = signed(FEED);
    // Another size, as a changed package would have.
    let at = feed.windows(8).position(|w| w == b"18889777").unwrap();
    feed[at] = b'2';

    assert!(matches!(verified(&feed, &signature, &key), Err(FeedError::Unsigned)));
}

#[test]
fn a_feed_signed_with_another_key_is_refused() {
    let (feed, signature, _) = signed(FEED);

    assert!(matches!(verified(&feed, &signature, &public(&key_pair())), Err(FeedError::Unsigned)));
}

#[test]
fn a_missing_or_cut_short_signature_is_refused() {
    let (feed, signature, key) = signed(FEED);

    assert!(matches!(verified(&feed, &[], &key), Err(FeedError::Unsigned)));
    assert!(matches!(verified(&feed, &signature[..63], &key), Err(FeedError::Unsigned)));
}

#[test]
fn nothing_is_read_from_a_feed_before_its_signature_checks_out() {
    let (feed, signature, key) = signed("not a feed");

    assert!(matches!(verified(b"not a feed either", &signature, &key), Err(FeedError::Unsigned)));
    // Signed, so it's read, and only then found not to be a feed.
    assert!(matches!(verified(&feed, &signature, &key), Err(FeedError::Unreadable(_))));
}

#[test]
fn a_key_decodes_from_base64() {
    let counting: [u8; 32] = std::array::from_fn(|i| u8::try_from(i).unwrap());

    assert_eq!(FeedKey::from_base64("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="), FeedKey(counting));
    assert_eq!(FeedKey::from_base64("//////////////////////////////////////////8=").0, [255; 32]);
}

/// Serves `files` from a port on this Mac, and a 404 for anything else, until the test process ends.
fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut request = [0; 4096];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or_default().trim_start_matches('/');
            let (status, body) = match files.iter().find(|(name, _)| *name == path) {
                Some((_, body)) => ("200 OK", body.clone()),
                None => ("404 Not Found", Vec::new()),
            };
            write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    base
}

#[test]
fn the_source_reads_a_signed_feed_and_refuses_one_without_its_signature() {
    let (feed, signature, key) = signed(FEED);
    let both = serve(vec![("releases.osx.json", feed.clone()), ("releases.osx.json.sig", signature)]);
    let feed_only = serve(vec![("releases.osx.json", feed)]);

    let read = SignedFeed::new(&format!("{both}/"), key).get_release_feed("osx", &Manifest::default(), "");
    let refused = SignedFeed::new(&feed_only, key).get_release_feed("osx", &Manifest::default(), "");

    assert_eq!(read.unwrap().Assets.len(), 1);
    assert!(matches!(refused, Err(velopack::Error::Other(_))));
}
