//! Conversation mode on this Mac: speech, the voice every conversation here reads with, Phonon, whose files are
//! fetched once speech is on, as `docs/spec/chats.md` says, and each hub's conversations, which its view follows from
//! the records whichever workspace shows, so one here goes on reading while its chat isn't showing. What to read, what
//! the mode needs and what the bar shows are `client-core`'s, and following them and the voice are the `conversation`
//! crate's, so the phone has the same. This only holds them for the app.

use std::path::Path;

use client_core::Hub;
use client_core::conversation::{Needs, Readiness};
use conversation::{Conversations, Voices};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::{App, AppContext as _, BorrowAppContext as _, Context, Entity, Global, Subscription, Window};

use crate::settings::Choice;
use crate::status_box::{Action, StatusBox, Tone};

/// Speech's box in Settings, and its button, for tests to find them by.
pub const SPEECH_STATUS: &str = "speech-status";
pub const SPEECH_RETRY: &str = "speech-retry";

/// This Mac's voice, and whether the person has speech on, as it last followed that.
struct Speech {
    voices: Voices,
    on: bool,
    _following_the_choice: Subscription,
}

impl Global for Speech {}

/// Has this Mac's voice keep its files in `data_dir`, and follow the person's choice of speech. Once, as the app
/// starts. Nothing is fetched until speech is on.
pub(crate) fn start(data_dir: &Path, cx: &mut App) {
    use_voices(Voices::new(data_dir.join("speaking").join("phonon"), "this Mac"), cx);
}

/// Has this Mac read with `voices`, as the person's choice of speech has it, with whatever shows how it stands hearing
/// as that changes.
fn use_voices(voices: Voices, cx: &mut App) {
    let changed = voices.changed().clone();
    cx.spawn(async move |cx| {
        while changed.recv_async().await.is_ok() {
            // Touched, so whatever observes it hears, and drawn again, as Settings and every bar show it.
            cx.update(|cx| {
                cx.update_global::<Speech, _>(|_, _| {});
                cx.refresh_windows();
            });
        }
    })
    .detach();
    let following = crate::settings::observe_in_app(cx, follow_the_choice);
    cx.set_global(Speech { voices, on: false, _following_the_choice: following });
    follow_the_choice(cx);
}

/// Turns speech on or off, and has it read at the speed, as the person chose, once either changes.
fn follow_the_choice(cx: &mut App) {
    let settings = crate::settings::get(cx);
    let on = settings.speech == Choice::On;
    cx.update_global::<Speech, _>(|speech, _| {
        speech.voices.set_speed(settings.speech_speed);
        if speech.on != on {
            speech.on = on;
            if on { speech.voices.turn_on() } else { speech.voices.turn_off() }
        }
    });
}

/// Tries getting the voice ready again, after it failed.
pub(crate) fn retry(cx: &App) {
    if let Some(voices) = voices(cx) {
        voices.turn_on();
    }
}

/// A hub's conversations on this Mac, with every window drawn again as what one here is reading changes.
pub(crate) fn conversations(hub: Hub, cx: &mut App) -> Entity<Conversations> {
    let conversations = Conversations::new(hub);
    let changed = conversations.changed().clone();
    cx.spawn(async move |cx| {
        while changed.recv_async().await.is_ok() {
            cx.update(App::refresh_windows);
        }
    })
    .detach();
    cx.new(|_| conversations)
}

/// This Mac's voice, as [`start`] made it, or `None` before it has, as in a test of something else.
pub(crate) fn voices(cx: &App) -> Option<&Voices> {
    cx.try_global::<Speech>().map(|speech| &speech.voices)
}

/// How dictation and speech stand on this Mac, which conversation mode needs both of.
pub(crate) fn needs(cx: &App) -> Needs {
    Needs {
        dictation: crate::dictating::for_conversation(cx),
        speech: voices(cx).map_or(Readiness::NotOn, Voices::ready),
    }
}

/// How big the voice is, as Settings and conversation mode's card say it, such as `65 MB`.
pub(crate) fn size() -> String {
    format!("{} MB", (conversation::FILES.size_bytes() + 500_000) / 1_000_000)
}

/// How speech stands, as a box under its setting, while it's on: the voice being downloaded, ready, or failed, with
/// Retry.
pub(crate) fn status_box(cx: &App) -> Option<StatusBox> {
    let said = |icon, tone, lead: &str, detail: String| StatusBox {
        id: SPEECH_STATUS,
        icon,
        tone,
        lead: Some(lead.to_string().into()),
        detail: detail.into(),
        progress: None,
        action: None,
    };
    Some(match voices(cx)?.ready() {
        Readiness::NotOn => return None,
        Readiness::Downloading(done) => {
            let detail = done.map_or_else(String::new, |done| format!("{:.0}% of {}.", done * 100., size()));
            StatusBox { progress: done, ..said(Lucide::Download, Tone::Busy, "Downloading the voice.", detail) }
        }
        Readiness::Preparing => said(Lucide::LoaderCircle, Tone::Busy, "Preparing the voice.", String::new()),
        Readiness::Ready => {
            said(Lucide::CircleCheck, Tone::Done, "Ready.", format!("The voice is on this Mac, {}.", size()))
        }
        Readiness::Failed(why) => {
            let retry = Action { id: SPEECH_RETRY, label: "Retry", off: false, act: |cx| retry(cx) };
            StatusBox {
                action: Some(retry),
                ..said(Lucide::TriangleAlert, Tone::Problem, "Couldn't download the voice:", why)
            }
        }
    })
}

/// Whether this Mac offers conversation mode: it can hear, and neither dictation nor speech is off.
pub(crate) fn offered(cx: &App) -> bool {
    let settings = crate::settings::get(cx);
    crate::dictating::runs_here(cx) && settings.dictation != Choice::Off && settings.speech != Choice::Off
}

/// Has `view` hear, in `window`, whenever how dictation or speech stands changes.
pub(crate) fn observe_needs<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    changed: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
) -> [Subscription; 2] {
    let speech = changed.clone();
    [
        crate::dictating::observe_in(window, cx, changed),
        cx.observe_global_in::<Speech>(window, move |view, window, cx| speech(view, window, cx)),
    ]
}

/// Has this Mac's voice keep its files in `model_dir`, fetched from `from`, such as a test's server, rather than
/// Hugging Face, and turns speech on, which starts getting it ready.
#[cfg(any(test, feature = "testing"))]
pub fn fetch_the_voice_from(model_dir: &Path, from: String, cx: &mut App) {
    use_voices(Voices::fetched_from(model_dir.to_path_buf(), "this Mac", from), cx);
    crate::settings::change(cx, |settings| settings.speech = Choice::On);
}

/// Has this Mac read with a voice that has nothing to fetch, so it's ready as soon as speech is on, and reads nothing,
/// for a test of conversation mode that has no use for what's read out.
#[cfg(any(test, feature = "testing"))]
pub fn use_a_silent_voice(cx: &mut App) {
    use_voices(Voices::silent(), cx);
}

/// Turns speech on, or off, as the person can in Settings.
#[cfg(any(test, feature = "testing"))]
pub fn set_speech_on(on: bool, cx: &mut App) {
    let choice = if on { Choice::On } else { Choice::Off };
    crate::settings::change(cx, |settings| settings.speech = choice);
}

/// Whether dictation and speech are both ready, so conversation mode can start.
#[cfg(any(test, feature = "testing"))]
pub fn is_ready(cx: &App) -> bool {
    needs(cx).ready()
}

/// How speech stands on this Mac.
#[cfg(any(test, feature = "testing"))]
pub fn speech(cx: &App) -> Option<Readiness> {
    voices(cx).map(Voices::ready)
}
