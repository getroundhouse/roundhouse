//! Dictating into the composer of the chat showing: starting and stopping, the live text under what's typed, final text
//! going in at the cursor, sending once it has, Esc putting the prompt back, and the recording ending once its composer
//! isn't drawn. The pane holds the recording, so it ends with the pane too.

use dictation::{DictationError, Ending, Permission, Recording, Update};
use gpui_kit::component::Size;
use gpui_kit::component::input::{Position, Rope, TextareaState};
use gpui_kit::{
    App, Context, InteractiveElement, IntoElement, ParentElement as _, Role, StatefulInteractiveElement as _,
    Styled as _, Task, TestSupportExt as _, Window, div,
};
use protocol::model::SessionId;
use protocol::prompt::says_or_carries;

use super::{PaneEvent, Showing, WorkspacePane};
use crate::{CancelDictation, HoldToDictate, dictating};

/// The line of live text under what's typed, as tests find it.
pub const LIVE_LINE: &str = "dictation-live";

/// What the person is told of a recording that heard nothing.
pub const NOTHING_HEARD: &str = "No speech was heard.";

/// Where the person allows the app to use the microphone, which only they can change.
const ALLOW_THE_MICROPHONE: &str =
    "Allow Roundhouse to use the microphone in System Settings › Privacy & Security › Microphone, then try again.";

/// What's being dictated into the composer, from its start until it has said all it will. Dropping it cancels the
/// recording, unless it was stopped, and nothing more of it is heard.
pub(super) struct Underway {
    /// The chat whose composer it goes into.
    chat: SessionId,
    started: Started,
    stage: Stage,
    /// The live text of what's being said, dimmed under what's typed.
    live: String,
    /// What the composer had when it started, which Esc puts back.
    before: Before,
    /// What's to be done once it has said all it will, as Enter asked for while it was being dictated.
    then: Option<Then>,
    recording: Recording,
    /// Hears what the recording says until it says no more. It goes with the recording, so it only runs while its
    /// recording is the pane's.
    _hearing: Task<()>,
}

/// How a recording was started, which says what stops it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Started {
    /// By holding Space, with `Option` or long enough without it, so letting go of it stops it.
    ByTheKey,
    /// By the microphone button, which stops it when pressed again.
    ByTheButton,
}

/// Sending or queueing, asked for while something was being dictated, and done once the last of it has gone in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Then {
    Send,
    Queue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Hearing,
    /// Stopped, and waiting for the final text of what was said last.
    Stopping,
}

/// What a composer had in it, and where its cursor was. The text is the kit's rope, so keeping it copies nothing.
pub(super) struct Before {
    text: Rope,
    cursor: Position,
}

impl Before {
    /// What `composer` has now.
    pub(super) fn of(composer: &TextareaState) -> Self {
        Self { text: composer.text().clone(), cursor: composer.cursor_position() }
    }
}

impl WorkspacePane {
    /// Whether the composer of the chat showing can be dictated to: dictation is ready, and the composer is drawn.
    pub(super) fn can_dictate(&self, cx: &App) -> bool {
        dictation::HEARS && dictating::ready(cx) && self.composer_drawn(cx).is_some()
    }

    /// The chat whose composer is drawn: this pane is, the chat isn't behind the file tab or a subagent's, and no
    /// question takes the composer's place. Another app in front of the window doesn't change it.
    fn composer_drawn(&self, cx: &App) -> Option<&SessionId> {
        let question = self.transcript().is_some_and(|t| t.read(cx).question().is_some());
        self.active.as_ref().filter(|_| self.drawn() && self.showing() == Showing::Chat && !question)
    }

    /// Starts dictating into the chat showing. The first time, the system asks whether the app may use the microphone,
    /// and dictation starts with the next press once it may.
    pub(super) fn dictate(&mut self, started: Started, window: &mut Window, cx: &mut Context<Self>) {
        if self.dictation.is_some() || !self.can_dictate(cx) {
            return;
        }
        // Space held towards dictating has nothing left to start, and what it was hearing ends before this begins.
        self.forget_space_down();
        if dictating::hears_the_microphone(cx) {
            match dictation::microphone_permission() {
                Permission::Allowed => {}
                Permission::NotAsked => return dictation::ask_for_microphone(|_| {}),
                Permission::Denied => return cx.emit(PaneEvent::Error(ALLOW_THE_MICROPHONE.to_string())),
            }
        }
        if let Some((recording, updates)) = dictating::record(cx) {
            self.dictate_with(recording, updates, started, window, cx);
        }
    }

    /// Dictates what `recording` hears into the chat showing, from what's heard of it, `updates`, such as what Space
    /// held has heard since it began to listen.
    pub(super) fn dictate_with(
        &mut self,
        recording: Recording,
        updates: flume::Receiver<Update>,
        started: Started,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dictation.is_some() || !self.can_dictate(cx) {
            return;
        }
        let Some(chat) = self.active.clone() else { return };
        let before = Before::of(self.composer.read(cx));
        let hearing = cx.spawn_in(window, async move |this, cx| {
            while let Ok(update) = updates.recv_async().await {
                if this.update_in(cx, |t, window, cx| t.dictated(update, window, cx)).is_err() {
                    return;
                }
            }
            let _ = this.update_in(cx, Self::dictation_said_all);
        });
        let live = String::new();
        let (stage, then) = (Stage::Hearing, None);
        self.dictation = Some(Underway { chat, started, stage, live, before, then, recording, _hearing: hearing });
        cx.notify();
    }

    /// Stops what's being dictated, so what was said last turns final.
    fn stop_dictating(&mut self, cx: &mut Context<Self>) {
        if let Some(underway) = self.dictation.as_mut().filter(|u| u.stage == Stage::Hearing) {
            underway.stage = Stage::Stopping;
            underway.recording.stop();
            cx.notify();
        }
    }

    /// Space has been let go of, which stops what holding it started, and leaves the space it typed if it hadn't yet.
    pub(crate) fn dictation_key_let_go(&mut self, cx: &mut Context<Self>) {
        self.forget_space_down();
        if self.dictation.as_ref().is_some_and(|u| u.started == Started::ByTheKey) {
            self.stop_dictating(cx);
        }
    }

    /// Another app has come in front, which stops what's being dictated, as letting go of the key does: a key held to
    /// dictate is let go of there, where the app can't hear it, and the microphone shouldn't stay on while nobody's
    /// looking at what it puts in. What was said goes in.
    pub(crate) fn dictation_window_left(&mut self, cx: &mut Context<Self>) {
        self.forget_space_down();
        self.stop_dictating(cx);
    }

    /// The microphone button: starts dictating, or stops what's being dictated.
    pub(super) fn toggle_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dictation.is_some() {
            self.stop_dictating(cx);
        } else {
            self.dictate(Started::ByTheButton, window, cx);
        }
    }

    /// Esc while dictating: stops at once, and puts back what the composer had when it started.
    fn cancel_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Underway { before, .. }) = self.dictation.take() else { return };
        self.put_back(&before, window, cx);
        cx.notify();
    }

    /// Puts back what the composer had, with its cursor where it was.
    pub(super) fn put_back(&mut self, before: &Before, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| {
            c.replace_all(before.text.to_string(), window, cx);
            c.set_cursor_position(before.cursor, window, cx);
        });
        self.keep_typed(false, cx);
    }

    /// Ends what's being dictated once its composer isn't drawn, dropping what wasn't final, and forgets Space held
    /// there.
    pub(super) fn dictation_follows_its_composer(&mut self, cx: &mut Context<Self>) {
        let drawn = self.composer_drawn(cx).cloned();
        self.space_down_follows_its_composer(drawn.as_ref());
        if self.dictation.as_ref().is_some_and(|underway| drawn.as_ref() != Some(&underway.chat)) {
            self.dictation = None;
            cx.notify();
        }
    }

    /// Ends what's being dictated, such as once what was in the composer has been sent, when there's nothing left for
    /// Esc to put back.
    pub(super) fn end_dictation(&mut self) {
        self.dictation = None;
    }

    /// Enter, Send or queueing while something's being dictated: stops it, and sends or queues once the last of what was
    /// said has gone in, so none of it is lost. With nothing typed or heard yet, there's nothing to send, and it goes on
    /// being dictated. Whether something was, which leaves nothing to do now.
    pub(super) fn once_dictated(&mut self, then: Then, cx: &mut Context<Self>) -> bool {
        let typed = says_or_carries(&self.composer.read(cx).text().to_string(), self.attached.len());
        let Some(underway) = self.dictation.as_mut() else { return false };
        if typed || !underway.live.is_empty() || underway.stage == Stage::Stopping {
            underway.then = Some(then);
            self.stop_dictating(cx);
        }
        true
    }

    /// What's heard of what's being dictated: its live text to draw, its final text to put in at the cursor, and how it
    /// ended. It stays the pane's until it has said all it will.
    fn dictated(&mut self, update: Update, window: &mut Window, cx: &mut Context<Self>) {
        let Some(underway) = self.dictation.as_mut() else { return };
        match update {
            Update::Hearing(text) => underway.live = text,
            Update::Final(text) => self.put_in(&text, window, cx),
            Update::Ended(Ending::Heard) => {}
            Update::Ended(Ending::NothingHeard) => cx.emit(PaneEvent::Error(NOTHING_HEARD.to_string())),
            // What went in stays in the composer, to be sent once the person has seen what's missing.
            Update::Failed(e) => {
                underway.then = None;
                cx.emit(PaneEvent::Error(failure(&e)));
            }
        }
        cx.notify();
    }

    /// What's being dictated has said all it will, whether or not it said how it ended, so its line goes, and what Enter
    /// asked for meanwhile is done.
    fn dictation_said_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.dictation.take().and_then(|underway| underway.then) {
            Some(Then::Send) => self.submit(window, cx),
            Some(Then::Queue) => self.queue_typed(window, cx),
            None => {}
        }
        cx.notify();
    }

    /// Puts `text` in the composer at the cursor, as if typed, with a space between it and what's on either side.
    fn put_in(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| {
            let typed = c.text().to_string();
            let (before, after) = (typed.get(..c.cursor()).unwrap_or(&typed), typed.get(c.cursor()..).unwrap_or(""));
            let apart = |next: Option<char>| if next.is_some_and(|c| !c.is_whitespace()) { " " } else { "" };
            let text = format!("{}{text}{}", apart(before.chars().last()), apart(after.chars().next()));
            c.insert(text, window, cx);
        });
        self.keep_typed(false, cx);
    }

    /// Whether anything is being dictated into this pane's composer, for a test to see it end while nothing draws it.
    #[cfg(feature = "testing")]
    pub fn dictating(&self) -> bool {
        self.dictation.is_some()
    }

    /// Gives what's being dictated `audio`, at 16 kHz, as a microphone would, a buffer at a time, or what Space held
    /// is hearing towards dictating.
    #[cfg(feature = "testing")]
    pub fn hear(&self, audio: &[f32]) {
        let recording = self.dictation.as_ref().map(|underway| &underway.recording);
        if let Some(recording) = recording.or_else(|| self.space_listening()) {
            for buffer in audio.chunks(160) {
                recording.feed(buffer.to_vec());
            }
        }
    }
}

/// What to tell the person of a recording that failed.
fn failure(e: &DictationError) -> String {
    match e {
        DictationError::MicrophoneDenied => ALLOW_THE_MICROPHONE.to_string(),
        // Its permission was reset after the press checked it: the next press asks.
        DictationError::MicrophoneNotAsked => {
            "Dictate again, and allow Roundhouse to use the microphone when asked.".to_string()
        }
        DictationError::NoMicrophone => "There's no microphone to dictate with.".to_string(),
        DictationError::Microphone(_) | DictationError::Engine(_) => format!("Dictation stopped: {e}."),
    }
}

/// What's drawn of dictation in the composer.
impl WorkspacePane {
    /// `composer` hearing the keys held to dictate, which type what they would have where there's nothing to dictate
    /// to, and Esc, which cancels what's being dictated, or does what it would have where nothing is.
    pub(super) fn hears_dictation_keys<E: InteractiveElement>(composer: E, cx: &mut Context<Self>) -> E {
        let hold = cx.listener(|t, _: &HoldToDictate, window, cx| {
            if t.dictation.is_none() && !t.can_dictate(cx) {
                return cx.propagate();
            }
            // Space's repeats type nothing until it's let go of, even if dictating doesn't start, as when the system asks
            // first whether the app may use the microphone.
            dictating::held(cx);
            t.dictate(Started::ByTheKey, window, cx);
        });
        let cancel = cx.listener(|t, _: &CancelDictation, window, cx| {
            if t.dictation.is_none() {
                return cx.propagate();
            }
            t.cancel_dictation(window, cx);
        });
        // Plain Space is a key, not a binding, as it types; it's heard on its way in, before the text field.
        let key_down = cx.listener(Self::composer_key_down);
        composer.on_action(hold).on_action(cancel).capture_key_down(key_down)
    }

    /// The line under what's typed while it's being dictated to: the live text, dimmed, until it's final.
    pub(super) fn dictation_line(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let colors = look::colors(cx);
        let underway = self.dictation.as_ref()?;
        let (text, colour) = match (underway.live.as_str(), underway.stage) {
            ("", Stage::Hearing) => ("Listening…", colors.text_faint),
            ("", Stage::Stopping) => ("…", colors.text_faint),
            (live, _) => (live, colors.text_muted),
        };
        // A status, so a screen reader can say what's heard as it changes.
        let line = div().id(LIVE_LINE).role(Role::Status).aria_label(text.to_string()).test_support();
        // Inset as what's typed is, in the text area of the kit's usual size, so the two line up.
        let line = line.px(Size::default().input_px()).text_color(colour).child(text.to_string());
        Some(div().w_full().px(look::Space::Small).pt(look::Space::XSmall).child(line))
    }
}
