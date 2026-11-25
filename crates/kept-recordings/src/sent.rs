//! What was sent from a field that had been dictated into, kept beside its recordings, so what they came out as can be
//! set against what the person made of it.

use serde::{Deserialize, Serialize};

use crate::Field;

/// What was sent, in the recordings' folder: one JSON line for each thing sent, in the order they were.
pub const SENT: &str = "sent.jsonl";

/// Where a recording has been kept and nothing sent since, in the recordings' folder: a JSON list of [`SentFrom`].
/// There's none while nothing waits.
pub const UNSENT: &str = "unsent.json";

/// The form this build writes [`Sent`] in. A later build reads an earlier one's, which it knows by this.
pub const SENT_VERSION: u32 = 1;

/// Something sent from where a recording had been kept since the last thing sent from there. Which recordings it came
/// from, and what was changed, isn't here: whatever reads it works that out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    pub version: u32,
    /// When it was sent, in milliseconds since 1970, in UTC.
    pub sent_at_ms: u64,
    pub from: SentFrom,
    /// All that was sent, dictated or typed, as it was first sent, though the hub may have refused it.
    pub text: String,
}

/// Where something's sent from, as a recording's file says it was dictated there.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SentFrom {
    pub field: Field,
    /// The workspace, by its ID, unless it's for one not made yet.
    pub workspace: Option<String>,
    /// The chat, by its ID, if it's in one.
    pub chat: Option<String>,
    /// Whether it was said in conversation mode.
    pub said_out_loud: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Sent, SentFrom};

    /// What version 1 wrote, which every later build reads.
    #[test]
    fn version_1_is_read() {
        let from = json!({"field": "composer", "workspace": "w1", "chat": "s1", "said_out_loud": false});
        let written = json!({"version": 1, "sent_at_ms": 1_795_600_000_000_u64, "from": from, "text": "Fix it."});
        let sent: Sent = serde_json::from_value(written.clone()).unwrap();
        assert_eq!(serde_json::to_value(&sent).unwrap(), written);
        assert_eq!(serde_json::from_value::<SentFrom>(from).unwrap(), sent.from, "as what's unsent is written too");
    }
}
