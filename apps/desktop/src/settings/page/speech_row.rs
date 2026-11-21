//! The Speech row of General: Ask first, On or Off, and under it a box that says how the voice stands, as
//! `docs/spec/settings.md` says. Only where `sound::HEARS`, since conversation mode, which reads with it, needs
//! dictation too.

use super::{Control, Setting};

/// The button that opens the choices.
const CHOICE: &str = "settings-speech";

pub(super) fn setting() -> Setting {
    Setting {
        name: "Speech",
        does: "Hear the agent's replies read out in conversation mode. It needs a voice of 65 MB, downloaded once, \
               which speaks on this Mac. Ask first lets conversation mode offer to turn it on.",
        control: Control::ask_first(CHOICE, |settings| settings.speech, |settings, choice| settings.speech = choice),
        status_box: Some(Box::new(crate::conversing::status_box)),
    }
}

#[cfg(test)]
mod tests {
    use super::setting;

    #[test]
    fn what_it_says_it_downloads_is_what_there_is() {
        let size = crate::conversing::size();
        assert!(setting().does.contains(&size), "{size}");
    }
}
