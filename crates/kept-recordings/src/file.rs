//! What a kept dictation's JSON file says.

use hint_words::{HintWord, PickedFrom};
use serde::{Deserialize, Serialize};

/// The audio, in a dictation's folder.
pub const AUDIO: &str = "audio.wav";

/// What's said of it, in its folder.
pub const FILE: &str = "recording.json";

/// The form this build writes. A later build reads an earlier one's, which it knows by this. Version 1 didn't have
/// [`Kept::picked_from`].
pub const VERSION: u32 = 2;

/// A dictation someone chose to keep. It's written as the dictation starts, and again as it ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Kept {
    pub version: u32,
    /// Its ID, which the app's log has too.
    pub id: String,
    /// When it started, in milliseconds since 1970, in UTC.
    pub started_at_ms: u64,
    /// The build of the app that heard it.
    pub build: String,
    /// The revision of the model that heard it.
    pub model: String,
    /// The language it was heard in, by its code, or `auto`.
    pub language: String,
    /// How strongly its hints were favoured.
    pub hint_boost: f32,
    /// The words it was biased toward, in order.
    pub hints: Vec<HintWord>,
    /// What they were picked from, unless version 1 wrote it.
    pub picked_from: Option<PickedFrom>,
    pub field: Field,
    /// The workspace it was in, by its ID, unless it was for one not made yet.
    pub workspace: Option<String>,
    /// The chat it was in, by its ID, if it was in one.
    pub chat: Option<String>,
    /// Whether it was said in conversation mode.
    pub said_out_loud: bool,
    /// The microphone's device, once it opened.
    pub device: Option<Device>,
    /// What was written of it, in order, live and final.
    pub said: Vec<Said>,
    /// When it was stopped, and its last words were still to come, from its start.
    pub stopped_ms: Option<u64>,
    /// How it ended, or `None` if the app went before it did.
    pub ended: Option<Ended>,
}

/// The field it was dictated into.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Composer,
    NewWorkspace,
    DiffComment,
    Answer,
    ProposalNote,
}

/// The device it was heard through.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub name: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Something written of it, and when, from its start.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub at_ms: u64,
    pub text: Text,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Text {
    /// All that's been heard since the last sentence turned final.
    Live(String),
    /// One sentence's final text, or part of a long one's.
    Final(String),
}

/// How it ended, and when, from its start.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    pub at_ms: u64,
    pub how: How,
    /// Why it failed, if it did.
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum How {
    Heard,
    NothingHeard,
    /// The person cancelled it, and the field was put back as it was.
    Cancelled,
    /// Its field was hidden, or reused for something else, or another field dictated.
    FieldGone,
    Failed,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::Kept;

    /// What version 1 wrote, which every later build reads.
    #[test]
    fn version_1_is_read() {
        let kept: Kept = serde_json::from_value(version_1()).unwrap();
        assert_eq!(kept.picked_from, None);
        assert_eq!(
            without_version_2(serde_json::to_value(&kept).unwrap()),
            version_1(),
            "the rest is as it was written"
        );
    }

    /// What version 2 writes, which every later build reads.
    #[test]
    fn version_2_is_read() {
        let written = version_2();
        let kept: Kept = serde_json::from_value(written.clone()).unwrap();
        assert_eq!(serde_json::to_value(&kept).unwrap(), written);
    }

    fn version_1() -> Value {
        json!({
            "version": 1, "id": "r1", "started_at_ms": 1_795_600_000_000_u64, "build": "0.1.500", "model": "bd13ddde",
            "language": "en", "hint_boost": 2.0, "hints": [{"text": "sqlite-vec", "spelled": true}],
            "field": "composer", "workspace": "w1", "chat": "s1", "said_out_loud": false,
            "device": {"name": "MacBook Pro Microphone", "sample_rate": 48000, "channels": 1},
            "said": [{"at_ms": 900, "text": {"live": "fix the"}}, {"at_ms": 1800, "text": {"final": "Fix the test."}}],
            "stopped_ms": 1500, "ended": {"at_ms": 1900, "how": "heard", "error": null}
        })
    }

    /// Version 1's, with what version 2 added.
    fn version_2() -> Value {
        let asked = json!({
            "header": "Pooling", "text": "Keep idle links?",
            "options": [{"label": "Keep them", "description": "Uses `idle_ms`."}, {"label": "Drop", "description": null}]
        });
        let mut written = version_1();
        written["version"] = json!(2);
        written["picked_from"] = json!({
            "typed": "Ask about", "attached": ["notes.md"], "open_file": "src/relay.rs", "question": asked,
            "proposal": {"title": "Holding links", "questions": [asked], "markdown": ["`LinkKeeper` drops it."]},
            "comment_on": "src/tunnel.rs", "repos": ["signalbox"],
            "workspace": {
                "name": "Bindery", "branches": ["anselm/bindery"], "repo": "signalbox", "commented": ["src/relay.rs"],
                "pull_request": "Keep the relay up"
            },
            "changed": ["src/relay.rs"], "chat": [{"first": 1, "last": 200}, {"first": 301, "last": 412}]
        });
        written
    }

    fn without_version_2(mut written: Value) -> Value {
        written.as_object_mut().unwrap().remove("picked_from");
        written
    }
}
