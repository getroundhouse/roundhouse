//! The app's recording source and the one field hearing it. Model setup and settings stay with the app.

use dictation::{Hints, Recording, Update};
use gpui_kit::{App, Context, Global, WeakEntity};
use sound::Quieting;

use crate::{About, Field, Keeping, Sent, Taken};

type Record = fn(Hints, &App) -> Option<Listening>;

/// A recording, as the app starts it.
pub struct Listening {
    pub recording: Recording,
    pub updates: flume::Receiver<Update>,
    /// What's taken of it to be kept, if the person chose to keep what they dictate.
    pub taken: Option<Taken>,
}

/// Installed once by the app, whose model setup answers these without I/O. An app without a source has no dictation.
pub struct Source {
    pub ready: fn(&App) -> bool,
    pub record: Record,
    pub uses_microphone: fn(&App) -> bool,
    /// Whether a recording quiets what's playing in the way it's given, while it's dictated to, as the person has
    /// chosen.
    pub quiets: fn(Quieting, &App) -> bool,
    /// Keeps a recording the app took what's kept of.
    pub keep: fn(Keeping, &mut App),
    /// Hears of what's sent from a field, whether or not any of it was dictated.
    pub sent: fn(Sent, &mut App),
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
        // Quitting drops no field, so this ends the one dictated to, which puts back what it quieted, and waits for
        // that, as long as quitting waits for anything.
        cx.on_app_quit(|cx| {
            unavailable(cx);
            sound::restored()
        })
        .detach();
    }
    cx.set_global(source);
}

pub(crate) fn ready(cx: &App) -> bool {
    sound::HEARS && cx.try_global::<Source>().is_some_and(|source| (source.ready)(cx))
}

pub(crate) fn uses_microphone(cx: &App) -> bool {
    cx.try_global::<Source>().is_some_and(|source| (source.uses_microphone)(cx))
}

pub(crate) fn quiets(how: Quieting, cx: &App) -> bool {
    cx.try_global::<Source>().is_some_and(|source| (source.quiets)(how, cx))
}

/// A recording, with what's heard of it, and what it's about.
pub(crate) struct Recorded {
    pub(crate) listening: Listening,
    pub(crate) about: About,
}

pub(crate) fn record(about: About, cx: &App) -> Option<Recorded> {
    let listening = (cx.try_global::<Source>()?.record)(about.hints.clone(), cx)?;
    Some(Recorded { listening, about })
}

pub(crate) fn keep(keeping: Keeping, cx: &mut App) {
    if let Some(source) = cx.try_global::<Source>() {
        (source.keep)(keeping, cx);
    }
}

pub(crate) fn sent(sent: Sent, cx: &mut App) {
    if let Some(source) = cx.try_global::<Source>() {
        (source.sent)(sent, cx);
    }
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
#[cfg(any(test, feature = "testing"))]
pub fn hear(audio: &[f32], cx: &App) {
    if let Some(field) = active(cx).and_then(|field| field.upgrade()) {
        field.read(cx).hear(audio);
    }
}

/// Whether what's being dictated quiets what's playing `how` says, or a recording listening towards held Space does.
#[cfg(any(test, feature = "testing"))]
pub fn quiets_while_dictating(how: Quieting, cx: &App) -> bool {
    active(cx).and_then(|field| field.upgrade()).is_some_and(|field| field.read(cx).quiets(how))
}
