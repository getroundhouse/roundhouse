//! The feed's signature, and where the published feed is, which the app and a hub installed as a service both read.
//!
//! This is the one definition of the signature, which `tools/package/feed-signature.sh` makes and the release workflow
//! checks. It's Ed25519 over the feed's exact bytes, as served, and the raw 64 bytes of it are beside the feed, in
//! `releases.<channel>.json.sig`. The public key is the raw 32 bytes, in base64. `fixture/` holds a feed that OpenSSL
//! signed with a throwaway key, which the tests here and `tools/package/test_publish.py` both check.

mod key;
mod published;
#[cfg(test)]
mod tests;

pub use key::{Unsigned, UpdateFeedKey};
pub use published::{HUB_CHANNEL, KEY, URL};
