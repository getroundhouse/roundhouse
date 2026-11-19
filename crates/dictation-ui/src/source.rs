//! The app's recording source and the one field hearing it. Model setup and settings stay with the app.

use dictation::{Hints, Recording, Update};
use gpui_kit::{App, Context, Global, WeakEntity};

use crate::Field;

type Record = fn(Hints, &App) -> Option<(Recording, flume::Receiver<Update>)>;

/// Installed once by the app, whose model setup answers these without I/O. An app without a source has no dictation.
pub struct Source {
    pub ready: fn(&App) -> bool,
    pub record: Record,
    pub uses_microphone: fn(&App) -> bool,
    /// Whether a recording mutes the sound output while it's dictated to, as the person has chosen.
    pub mutes_output: fn(&App) -> bool,
}

impl Global for Source {}

#[derive(Default)]
struct Active(Option<WeakEntity<Field>>);
impl Global for Active {}

/// Gives every opted-in field the same recording source and installs key-release handling around every window,
/// including the dialogs the kit draws beside the app's view.
pub fn install(source: Source, cx: &mut App) {
    if !cx.has_global::<Source>() {
        crate::keys::install(cx);
        // Quitting drops no field, so this ends the one dictated to, which unmutes what it muted, and waits for the
        // sound to be unmuted, as long as quitting waits for anything.
        cx.on_app_quit(|cx| {
            unavailable(cx);
            let restored = dictation::output_restored();
            async move {
                let _ = restored.recv_async().await;
            }
        })
        .detach();
    }
    cx.set_global(source);
}

pub(crate) fn ready(cx: &App) -> bool {
    dictation::HEARS && cx.try_global::<Source>().is_some_and(|source| (source.ready)(cx))
}

pub(crate) fn uses_microphone(cx: &App) -> bool {
    cx.try_global::<Source>().is_some_and(|source| (source.uses_microphone)(cx))
}

pub(crate) fn mutes_output(cx: &App) -> bool {
    cx.try_global::<Source>().is_some_and(|source| (source.mutes_output)(cx))
}

pub(crate) fn record(hints: Hints, cx: &App) -> Option<(Recording, flume::Receiver<Update>)> {
    (cx.try_global::<Source>()?.record)(hints, cx)
}

/// Ends an old target before asking the engine for another recording, including one still listening towards a held
/// Space. The old field keeps finalized text and drops its task, so late updates cannot reach the new target.
pub(crate) fn claim(cx: &mut Context<Field>) {
    let this = cx.weak_entity();
    let old = cx.default_global::<Active>().0.replace(this.clone());
    if let Some(old) = old.filter(|old| *old != this) {
        let _ = old.update(cx, Field::end);
    }
}

pub(crate) fn active(cx: &App) -> Option<WeakEntity<Field>> {
    cx.try_global::<Active>()?.0.clone()
}

/// Stops the current target when the app disables dictation or replaces its model. Dropping an engine's handle alone
/// lets an existing recording continue, so this happens before the app lets that handle go.
pub fn unavailable(cx: &mut App) {
    if let Some(field) = active(cx) {
        let _ = field.update(cx, Field::end);
    }
}

/// Gives what's being dictated synthetic audio, including a recording listening towards held Space, wherever it is,
/// since one field dictates at a time.
#[cfg(feature = "testing")]
pub fn hear(audio: &[f32], cx: &App) {
    if let Some(field) = active(cx).and_then(|field| field.upgrade()) {
        field.read(cx).hear(audio);
    }
}

/// Whether what's being dictated has the sound output muted, or a recording listening towards held Space has.
#[cfg(feature = "testing")]
pub fn mutes_output_while_dictating(cx: &App) -> bool {
    active(cx).and_then(|field| field.upgrade()).is_some_and(|field| field.read(cx).mutes_output())
}
