use super::*;
use crate::updates::serving::{Reply, Server, serve};
use crate::updates::signing::{key_pair, public};

/// The shape `vpk` writes, with one release.
const FEED: &str = r#"{"Assets":[{"PackageId":"Roundhouse","Version":"0.1.50","Type":"Full","FileName":"Roundhouse-0.1.50-osx-full.nupkg","SHA1":"AD4CF1813CB4589407AEEB7DA69063A25FF84566","SHA256":"0F568A995C5F5474207B3A3225F7B4059D2A268C35ED32310F1083E59CA286B7","Size":18889777}]}"#;

/// A feed, its signature, and the key that checks it, as the release workflow makes them.
fn signed(feed: &str) -> (Vec<u8>, Vec<u8>, UpdateFeedKey) {
    let pair = key_pair();
    (feed.as_bytes().to_vec(), pair.sign(feed.as_bytes()).as_ref().to_vec(), public(&pair))
}

/// Serves a feed and its signature, each as a list of replies in turn.
fn serve_feed(feed: Vec<Reply>, signature: Vec<Reply>) -> Server {
    serve(vec![("releases.osx.json".to_string(), feed), ("releases.osx.json.sig".to_string(), signature)])
}

/// Reads the feed through Velopack's interface, without waiting to read it again.
fn read_feed(server: &Server, key: UpdateFeedKey) -> Result<VelopackAssetFeed, velopack::Error> {
    SignedSource::waiting(&server.base, key, Duration::ZERO).get_release_feed("osx", &Manifest::default(), "")
}

/// The source's own error, which Velopack passes on inside its `Io` variant.
fn source_error<T: std::fmt::Debug>(result: Result<T, velopack::Error>) -> SourceError {
    match result {
        Err(velopack::Error::Io(io)) => *io.into_inner().unwrap().downcast::<SourceError>().unwrap(),
        other => panic!("not the source's error: {other:?}"),
    }
}

fn asked_for_the_feed(server: &Server) -> usize {
    server.asked().iter().filter(|path| *path == "releases.osx.json").count()
}

/// OpenSSL signed this feed with a throwaway key, as `tools/package/feed-signature.sh` signs the real one, and
/// `tools/package/test_publish.py` checks the same files, so the two sides can't disagree on the signature's form.
#[test]
fn a_feed_openssl_signed_is_read() {
    let key = UpdateFeedKey::from_base64(include_str!("fixture/throwaway-key.pub").trim_ascii());
    let feed = include_bytes!("fixture/releases.osx.json");

    let releases = verified(feed, include_bytes!("fixture/releases.osx.json.sig"), &key).unwrap();

    assert_eq!(releases.Assets[0].Version, "0.1.1");
}

#[test]
fn a_feed_signed_with_the_key_is_read() {
    let (feed, signature, key) = signed(FEED);

    let releases = verified(&feed, &signature, &key).unwrap();

    assert_eq!(releases.Assets.len(), 1);
    assert_eq!(releases.Assets[0].Version, "0.1.50");
}

#[test]
fn a_changed_feed_is_refused() {
    let (mut feed, signature, key) = signed(FEED);
    // Another size, as a changed package would have.
    let at = feed.windows(8).position(|w| w == b"18889777").unwrap();
    feed[at] = b'2';

    assert!(matches!(verified(&feed, &signature, &key), Err(SourceError::Unsigned)));
}

#[test]
fn a_feed_signed_with_another_key_is_refused() {
    let (feed, signature, _) = signed(FEED);

    assert!(matches!(verified(&feed, &signature, &public(&key_pair())), Err(SourceError::Unsigned)));
}

#[test]
fn a_missing_or_cut_short_signature_is_refused() {
    let (feed, signature, key) = signed(FEED);

    assert!(matches!(verified(&feed, &[], &key), Err(SourceError::Unsigned)));
    assert!(matches!(verified(&feed, &signature[..63], &key), Err(SourceError::Unsigned)));
}

#[test]
fn nothing_is_read_from_a_feed_before_its_signature_checks_out() {
    let (feed, signature, key) = signed("not a feed");

    assert!(matches!(verified(b"not a feed either", &signature, &key), Err(SourceError::Unsigned)));
    // Signed, so it's read, and only then found not to be a feed.
    assert!(matches!(verified(&feed, &signature, &key), Err(SourceError::Unreadable(_))));
}

#[test]
fn a_key_decodes_from_base64() {
    let counting: [u8; 32] = std::array::from_fn(|i| u8::try_from(i).unwrap());

    assert_eq!(UpdateFeedKey::from_base64("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="), UpdateFeedKey(counting));
    assert_eq!(UpdateFeedKey::from_base64("//////////////////////////////////////////8=").0, [255; 32]);
}

#[test]
fn the_source_reads_a_feed_signed_with_the_key() {
    let (feed, signature, key) = signed(FEED);
    let server = serve_feed(vec![Reply::file(feed)], vec![Reply::file(signature)]);

    assert_eq!(read_feed(&server, key).unwrap().Assets.len(), 1);
    assert_eq!(server.asked(), ["releases.osx.json", "releases.osx.json.sig"]);
}

#[test]
fn the_source_refuses_a_feed_signed_with_another_key_after_reading_it_again() {
    let (feed, signature, _) = signed(FEED);
    let server = serve_feed(vec![Reply::file(feed)], vec![Reply::file(signature)]);

    assert!(matches!(source_error(read_feed(&server, public(&key_pair()))), SourceError::Unsigned));
    assert_eq!(asked_for_the_feed(&server), 2);
}

#[test]
fn the_source_refuses_a_feed_changed_after_it_was_signed() {
    let (feed, signature, key) = signed(FEED);
    let changed = String::from_utf8(feed).unwrap().replace("0.1.50", "9.1.50");
    let server = serve_feed(vec![Reply::file(changed)], vec![Reply::file(signature)]);

    assert!(matches!(source_error(read_feed(&server, key)), SourceError::Unsigned));
}

/// The release workflow uploads the feed, then its signature, so a check can come between them.
#[test]
fn the_source_reads_again_a_feed_whose_signature_was_still_on_its_way() {
    let pair = key_pair();
    let new_feed = FEED.replace("0.1.50", "0.1.51");
    let old_signature = pair.sign(FEED.as_bytes()).as_ref().to_vec();
    let new_signature = pair.sign(new_feed.as_bytes()).as_ref().to_vec();
    let server = serve_feed(vec![Reply::file(new_feed)], vec![Reply::file(old_signature), Reply::file(new_signature)]);

    assert_eq!(read_feed(&server, public(&pair)).unwrap().Assets[0].Version, "0.1.51");
    assert_eq!(asked_for_the_feed(&server), 2);
}

#[test]
fn a_signature_is_checked_on_the_feed_as_decompressed() {
    let (feed, signature, key) = signed(FEED);
    let server = serve_feed(vec![Reply::gzipped(&feed)], vec![Reply::file(signature)]);

    assert_eq!(read_feed(&server, key).unwrap().Assets.len(), 1);
}

#[test]
fn a_feed_longer_than_the_limit_once_decompressed_is_refused() {
    let (_, signature, key) = signed(FEED);
    let long = vec![b' '; usize::try_from(FEED_LIMIT_BYTES).unwrap() + 1];
    let server = serve_feed(vec![Reply::gzipped(&long)], vec![Reply::file(signature)]);

    assert!(matches!(source_error(read_feed(&server, key)), SourceError::TooLong { .. }));
}

#[test]
fn a_redirect_is_not_followed() {
    let (feed, signature, key) = signed(FEED);
    let elsewhere = serve_feed(vec![Reply::file(feed)], vec![Reply::file(signature)]);
    let server = serve_feed(vec![Reply::redirect(&format!("{}/releases.osx.json", elsewhere.base))], vec![]);

    assert!(matches!(source_error(read_feed(&server, key)), SourceError::Status { status: 302, .. }));
    assert!(elsewhere.asked().is_empty());
}

/// A package the feed says is `size` bytes, served as `reply`, downloaded to a file of its own.
fn download(size: u64, reply: Reply) -> (Result<(), velopack::Error>, tempfile::TempDir) {
    let name = "Roundhouse-0.1.50-osx-full.nupkg";
    let server = serve(vec![(name.to_string(), vec![reply])]);
    let asset = VelopackAsset { FileName: name.to_string(), Size: size, ..VelopackAsset::default() };
    let dir = tempfile::tempdir().unwrap();
    let source = SignedSource::new(&server.base, public(&key_pair()));
    (source.download_release_entry(&asset, &dir.path().join("package.partial"), None), dir)
}

#[test]
fn a_package_downloads_as_long_as_the_feed_says() {
    let (downloaded, dir) = download(5, Reply::gzipped(b"12345"));

    downloaded.unwrap();
    assert_eq!(std::fs::read(dir.path().join("package.partial")).unwrap(), b"12345");
}

#[test]
fn a_package_longer_than_the_feed_says_once_decompressed_is_refused_and_not_kept() {
    let (downloaded, dir) = download(5, Reply::gzipped(&vec![b'0'; 10 * 1024 * 1024]));

    assert!(matches!(source_error(downloaded), SourceError::TooLong { limit: 5, .. }));
    assert!(!dir.path().join("package.partial").exists());
}

#[test]
fn a_package_cut_short_is_not_kept() {
    let (downloaded, dir) = download(6, Reply::file("12345"));

    assert!(matches!(source_error(downloaded), SourceError::CutShort { expected: 6, got: 5, .. }));
    assert!(!dir.path().join("package.partial").exists());
}
