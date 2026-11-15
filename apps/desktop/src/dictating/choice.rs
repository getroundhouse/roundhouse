//! Whether the composer can be dictated to, as the person chose: in Settings, or from the composer's microphone.

use serde::{Deserialize, Serialize};

/// Whether the composer can be dictated to. Kept in the app's settings as `"ask_first"`, `"on"` or `"off"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DictationChoice {
    /// The model is downloaded and prepared, and the composer can be dictated to once it's ready.
    On,
    /// The composer has no microphone.
    Off,
    /// Until the person chooses: the composer's microphone shows, and offers to turn dictation on, which downloads
    /// the model, so nothing is downloaded without asking. A choice this build doesn't know, saved by a newer one,
    /// reads as this, for the same reason, which is why it's last, as serde needs.
    #[default]
    #[serde(other)]
    AskFirst,
}

impl DictationChoice {
    /// Every choice, in the order Settings lists them.
    pub(crate) const ALL: [Self; 3] = [Self::AskFirst, Self::On, Self::Off];

    /// What Settings calls it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::AskFirst => "Ask first",
            Self::On => "On",
            Self::Off => "Off",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DictationChoice;

    #[test]
    fn its_kept_by_name_and_one_this_build_doesnt_know_asks_first() {
        let kept = |choice| serde_json::to_string(&choice).unwrap();
        assert_eq!(kept(DictationChoice::AskFirst), r#""ask_first""#);
        assert_eq!(kept(DictationChoice::Off), r#""off""#);
        let read = |text: &str| serde_json::from_str::<DictationChoice>(text).unwrap();
        assert_eq!(read(r#""on""#), DictationChoice::On);
        assert_eq!(read(r#""only_on_wednesdays""#), DictationChoice::AskFirst);
    }
}
