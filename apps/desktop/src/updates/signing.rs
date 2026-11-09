//! Keys for the tests, made as the update feed's is, and the form the app holds them in.

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair as _};

use super::source::UpdateFeedKey;

pub(crate) fn key_pair() -> Ed25519KeyPair {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
}

pub(crate) fn public(pair: &Ed25519KeyPair) -> UpdateFeedKey {
    UpdateFeedKey(pair.public_key().as_ref().try_into().unwrap())
}
