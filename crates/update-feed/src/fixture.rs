//! A feed that OpenSSL signed with a throwaway key, as `tools/package/feed-signature.sh` signs the real one, which
//! `tools/package/test_publish.py` checks too, so the two sides can't disagree on the signature's form or the fields.

use crate::UpdateFeedKey;

pub const FEED: &[u8] = include_bytes!("../fixture/releases.osx.json");
pub const SIGNATURE: &[u8] = include_bytes!("../fixture/releases.osx.json.sig");
pub const KEY: UpdateFeedKey = UpdateFeedKey::from_base64(include_str!("../fixture/throwaway-key.pub").trim_ascii());
