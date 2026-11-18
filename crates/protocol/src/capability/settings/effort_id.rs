//! An effort identifier supplied by an adapter. A newer CLI can add a level without a protocol change.

use serde::{Deserialize, Serialize};

/// Printable, with no spaces, and never the name of a level [`Effort`](super::Effort) has a variant for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(try_from = "String")]
pub struct EffortId(String);

impl EffortId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EffortId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || !value.bytes().all(|c| c.is_ascii_graphic()) {
            return Err("an effort ID must contain printable characters without spaces");
        }
        // So a level this build names has one form, its variant, and two copies of it are equal.
        if super::Effort::fixed(&value).is_some() {
            return Err("that effort has a variant of its own");
        }
        Ok(Self(value))
    }
}
