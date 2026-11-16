//! A field's recording and its lifetime. A host retains this beside its input, and tells it when that input is hidden
//! or reused for different content; being retained by a view does not mean it is still a target.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, Subscription, Window};

use crate::fine_line::FineLine;
use crate::holding::SpaceDown;
use crate::recording::Underway;

pub struct Field {
    pub(crate) line: Entity<FineLine>,
    pub(crate) input: Entity<TextareaState>,
    pub(crate) window: AnyWindowHandle,
    pub(crate) enabled: bool,
    pub(crate) underway: Option<Underway>,
    pub(crate) space_down: Option<SpaceDown>,
    _keys: Subscription,
    _input: Subscription,
}

/// Final text has already emitted the input's change event before an ending asks the host to submit it.
pub enum Event {
    /// The recording is gone. Only a successful finish requested by the host permits its pending action.
    Ended {
        submit: bool,
    },
    Error(String),
}

impl EventEmitter<Event> for Field {}

impl Field {
    pub fn new(input: Entity<TextareaState>, corner: look::Corner, window: &Window, cx: &mut Context<Self>) -> Self {
        let keys = cx.observe_keystrokes(|this, event, _, _| this.key_pressed(&event.keystroke));
        let changed = cx.subscribe(&input, |this, _, event: &InputEvent, _| {
            if matches!(event, InputEvent::Blur) {
                this.space_down = None;
            }
        });
        let line = cx.new(|cx| FineLine::new(corner, cx));
        Self {
            line,
            input,
            window: window.window_handle(),
            enabled: true,
            underway: None,
            space_down: None,
            _keys: keys,
            _input: changed,
        }
    }

    /// A hidden field cannot start or keep a recording, even when its view stays alive for reopening.
    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        if !enabled {
            self.end(cx);
        }
    }

    pub fn can_dictate(&self, cx: &App) -> bool {
        self.enabled && crate::source::ready(cx)
    }

    pub fn is_dictating(&self) -> bool {
        self.underway.is_some()
    }

    /// A form can submit before the first sentence is final; submission waits for those words.
    pub fn has_live_words(&self) -> bool {
        self.underway.as_ref().is_some_and(|u| !u.live.is_empty())
    }

    /// A hidden or repurposed input drops tentative words, keeping what already went in. It never performs a pending
    /// submission, and its cancelled task cannot deliver late text to a different question or chat.
    pub fn end(&mut self, cx: &mut Context<Self>) {
        self.space_down = None;
        if self.underway.take().is_some() {
            self.line.update(cx, FineLine::hide);
            cx.emit(Event::Ended { submit: false });
            cx.notify();
        }
    }

    /// True means the host must defer its action until `Ended { submit: true }`. With nothing typed, attached or heard,
    /// the key does nothing and dictation carries on; ending that recording will not submit. `can_submit` says the
    /// host can already act, including with attachments, selected options, or an empty new-workspace prompt.
    pub fn finish_before_submit(&mut self, can_submit: bool, cx: &mut Context<Self>) -> bool {
        let Some(underway) = self.underway.as_mut() else { return false };
        if can_submit || !underway.live.is_empty() || underway.stopping {
            underway.submit = true;
            self.stop(cx);
        }
        true
    }

    /// Gives the field synthetic audio, including to a recording listening towards held Space.
    #[cfg(feature = "testing")]
    pub fn hear(&self, audio: &[f32]) {
        let recording = self.underway.as_ref().map(|u| &u.recording).or_else(|| self.space_listening());
        if let Some(recording) = recording {
            for buffer in audio.chunks(160) {
                recording.feed(buffer.to_vec());
            }
        }
    }
}
