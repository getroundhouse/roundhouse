use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair as _};

use super::*;

/// A feed, its signature, and the key that checks it, as the release workflow makes them.
fn signed(feed: &[u8]) -> (Vec<u8>, UpdateFeedKey) {
    let pair = key_pair();
    (pair.sign(feed).as_ref().to_vec(), public(&pair))
}

fn key_pair() -> Ed25519KeyPair {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
}

fn public(pair: &Ed25519KeyPair) -> UpdateFeedKey {
    UpdateFeedKey(pair.public_key().as_ref().try_into().unwrap())
}

const FEED: &[u8] = br#"{"Assets":[{"Version":"0.1.50","Type":"Full","Size":18889777}]}"#;

#[test]
fn a_feed_openssl_signed_checks_out() {
    let checked = crate::fixture::KEY.verify(crate::fixture::FEED, crate::fixture::SIGNATURE);

    assert!(checked.is_ok(), "{checked:?}");
}

#[test]
fn a_changed_feed_is_refused() {
    let (signature, key) = signed(FEED);
    // Another size, as a changed package would have.
    let changed = String::from_utf8(FEED.to_vec()).unwrap().replace("18889777", "28889777");

    assert!(key.verify(FEED, &signature).is_ok());
    assert!(key.verify(changed.as_bytes(), &signature).is_err());
}

#[test]
fn a_feed_signed_with_another_key_is_refused() {
    let (signature, _) = signed(FEED);

    assert!(public(&key_pair()).verify(FEED, &signature).is_err());
}

#[test]
fn a_missing_or_cut_short_signature_is_refused() {
    let (signature, key) = signed(FEED);

    assert!(key.verify(FEED, &[]).is_err());
    assert!(key.verify(FEED, &signature[..63]).is_err());
}

#[test]
fn a_key_decodes_from_base64() {
    let counting: [u8; 32] = std::array::from_fn(|i| u8::try_from(i).unwrap());

    assert_eq!(UpdateFeedKey::from_base64("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="), UpdateFeedKey(counting));
    assert_eq!(UpdateFeedKey::from_base64("//////////////////////////////////////////8=").0, [255; 32]);
}

/// The key the release workflow signs with, which a build with no `ROUNDHOUSE_UPDATE_KEY` is given.
#[test]
fn the_published_feed_s_key_and_address_decode() {
    assert!(URL.starts_with("https://"), "{URL}");
    assert_ne!(KEY.0, [0; 32]);
}
