//! Whether something that needs a model downloaded is on, as the person chose: dictation, and speech. Each asks first
//! until it's chosen, since turning it on downloads its model.

use serde::{Deserialize, Serialize};

/// Whether dictation, or speech, is on. Kept in the app's settings as `"ask_first"`, `"on"` or `"off"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Choice {
    /// Its model is downloaded and prepared, and it works once it's ready.
    On,
    /// It's put away: for dictation, the composer has no microphone.
    Off,
    /// Until the person chooses: what uses it offers to turn it on, which downloads the model, so nothing is
    /// downloaded without asking. A choice this build doesn't know, saved by a newer one, reads as this, for the same
    /// reason, which is why it's last, as serde needs.
    #[default]
    #[serde(other)]
    AskFirst,
}

impl Choice {
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
    use super::Choice;

    #[test]
    fn its_kept_by_name_and_one_this_build_doesnt_know_asks_first() {
        let kept = |choice| serde_json::to_string(&choice).unwrap();
        assert_eq!(kept(Choice::AskFirst), r#""ask_first""#);
        assert_eq!(kept(Choice::Off), r#""off""#);
        let read = |text: &str| serde_json::from_str::<Choice>(text).unwrap();
        assert_eq!(read(r#""on""#), Choice::On);
        assert_eq!(read(r#""only_on_wednesdays""#), Choice::AskFirst);
    }
}
