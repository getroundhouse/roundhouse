//! What fields dictate with: this app's model, as it stands, and the person's choices.

use dictation::{AudioIn, Copied};
use dictation_ui::{Keeping, Listening, Taken};
use gpui_kit::App;

use super::keeping;
use super::readiness::{Hearing, Readiness, dictating, readiness, ready};

/// Whether a recording hears the microphone, which the system has to allow first, rather than audio a test gives it.
fn hears_the_microphone(cx: &App) -> bool {
    dictating(cx).audio == AudioIn::Microphone
}

/// Whether a recording quiets what's playing `how` says while it's dictated to: where this platform can, once it's
/// chosen.
fn quiets(how: sound::Quieting, cx: &App) -> bool {
    how.is_possible() && crate::settings::get(cx).quiets_while_dictating(how)
}

/// What fields dictate with: this app's model, as it stands, and the person's choices.
pub(crate) fn source() -> dictation_ui::Source {
    dictation_ui::Source { ready, record, uses_microphone: hears_the_microphone, quiets, keep, sent: keeping::sent }
}

/// Starts a recording biased toward `hints`, in the language chosen, once dictation is ready, and gives it and what's
/// heard of it, with what's kept of it if the person chose to keep what they dictate.
fn record(hints: dictation::Hints, cx: &App) -> Option<Listening> {
    let dictating = dictating(cx);
    match readiness(cx) {
        Readiness::Ready(Hearing(dictation)) => {
            let language = crate::settings::get(cx).dictation_language;
            if !keeping::keeps(cx) {
                let (recording, updates) = dictation.record(dictating.audio, hints, language);
                return Some(Listening { recording, updates, taken: None });
            }
            let Copied { recording, updates, audio } = dictation.record_copying(dictating.audio, hints, language);
            Some(Listening { recording, updates, taken: Some(Taken { audio, language }) })
        }
        Readiness::Off
        | Readiness::Downloading
        | Readiness::Preparing(_)
        | Readiness::Failed(_)
        | Readiness::NeedsNewerMacos => None,
    }
}

/// Keeps a recording that has begun to dictate.
fn keep(kept: Keeping, cx: &mut App) {
    let revision = dictating(cx).model.files.revision;
    keeping::keep(kept, revision, cx);
}
