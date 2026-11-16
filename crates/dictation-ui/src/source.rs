//! The app's recording source and the one field hearing it. Model setup and settings stay with the app.

use dictation::{Recording, Update};
use gpui_kit::{App, Context, Global, WeakEntity};

use crate::Field;

type Record = fn(&App) -> Option<(Recording, flume::Receiver<Update>)>;

/// Installed once by the app, whose model setup answers these without I/O. An app without a source has no dictation.
pub struct Source {
    pub ready: fn(&App) -> bool,
    pub record: Record,
    pub uses_microphone: fn(&App) -> bool,
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
    }
    cx.set_global(source);
}

pub(crate) fn ready(cx: &App) -> bool {
    dictation::HEARS && cx.try_global::<Source>().is_some_and(|source| (source.ready)(cx))
}

pub(crate) fn uses_microphone(cx: &App) -> bool {
    cx.try_global::<Source>().is_some_and(|source| (source.uses_microphone)(cx))
}

pub(crate) fn record(cx: &App) -> Option<(Recording, flume::Receiver<Update>)> {
    (cx.try_global::<Source>()?.record)(cx)
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
