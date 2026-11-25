//! A field's recording and its lifetime. A host retains this beside its input, and tells it when that input is hidden
//! or reused for different content; being retained by a view does not mean it is still a target.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, Subscription, Window};

use crate::fine_line::FineLine;
use crate::holding::SpaceDown;
use crate::place::Place;
use crate::recording::Underway;
use crate::{About, Sent, Within};

type AboutFn = Box<dyn Fn(&App) -> About>;

pub struct Field {
    pub(crate) place: Place,
    pub(crate) line: Entity<FineLine>,
    pub(crate) input: Entity<TextareaState>,
    pub(crate) window: AnyWindowHandle,
    pub(crate) enabled: bool,
    pub(crate) underway: Option<Underway>,
    pub(crate) space_down: Option<SpaceDown>,
    about: Option<AboutFn>,
    _keys: Subscription,
    _input: Subscription,
}

/// Final text has already emitted the input's change event before an ending asks the host to submit it.
pub enum Event {
    /// A recording began to dictate, once held Space has been held long enough, rather than as it starts to listen,
    /// so a press let go of sooner isn't taken for one.
    Started,
    /// The recording is gone. Only a successful finish requested by the host permits its pending action.
    Ended {
        submit: bool,
    },
    Error(String),
}

impl EventEmitter<Event> for Field {}

impl Field {
    /// Dictation over `input`, for the host view `cx` is of, which is drawn again as the field's live words and
    /// controls change, and which `on_event` tells of what happens to it. `about` gives what's dictated is about, with
    /// the words to favour, as each recording starts, and `enabled` whether the input shows now, as [`Self::set_enabled`] says. The host keeps
    /// the subscriptions as long as the field.
    pub fn attach<V: 'static>(
        input: &Entity<TextareaState>,
        place: Place,
        enabled: bool,
        about: impl Fn(&V, &App) -> About + 'static,
        on_event: impl Fn(&mut V, &Event, &mut Window, &mut Context<V>) + 'static,
        window: &Window,
        cx: &mut Context<V>,
    ) -> (Entity<Self>, [Subscription; 2]) {
        let host = cx.weak_entity();
        let field = cx.new(|cx| {
            let mut field = Self::new(input.clone(), place, window, cx);
            field.about =
                Some(Box::new(move |cx| host.upgrade().map_or_else(About::default, |host| about(host.read(cx), cx))));
            field.enabled = enabled;
            field
        });
        let events =
            cx.subscribe_in(&field, window, move |host, _, event, window, cx| on_event(host, event, window, cx));
        let changed = cx.observe(&field, |_, _, cx| cx.notify());
        (field, [events, changed])
    }

    fn new(input: Entity<TextareaState>, place: Place, window: &Window, cx: &mut Context<Self>) -> Self {
        let keys = cx.observe_keystrokes(|this, event, _, _| this.key_pressed(&event.keystroke));
        // The input is blurred as its window goes behind another app's too, which `keys` decides for itself.
        let changed = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, _| {
            if matches!(event, InputEvent::Blur) && window.is_window_active() {
                this.space_down = None;
            }
        });
        let line = cx.new(|cx| FineLine::new(place.corner(), cx));
        Self {
            place,
            line,
            input,
            window: window.window_handle(),
            enabled: true,
            underway: None,
            space_down: None,
            about: None,
            _keys: keys,
            _input: changed,
        }
    }

    pub(crate) fn current_about(&self, cx: &App) -> About {
        self.about.as_ref().map_or_else(About::default, |f| f(cx))
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

    /// Says that `text` was sent from this field, `within` there, as its host sends it, so it can be kept beside what
    /// was dictated into it. The host says only what the person wrote here.
    pub fn sent(&self, within: Within, text: String, cx: &mut Context<Self>) {
        crate::source::sent(Sent { place: self.place, within, text }, cx);
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

    /// Whether what's being dictated quiets what's playing the way `how` says, or a recording listening towards held
    /// Space does.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn quiets(&self, how: sound::Quieting) -> bool {
        self.recording().is_some_and(|recording| recording.quiets(how))
    }

    /// Gives the field synthetic audio, including to a recording listening towards held Space.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn hear(&self, audio: &[f32]) {
        if let Some(recording) = self.recording() {
            for buffer in audio.chunks(160) {
                recording.feed(buffer.to_vec());
            }
        }
    }

    /// What's being dictated, or listening towards held Space.
    #[cfg(any(test, feature = "testing"))]
    fn recording(&self) -> Option<&dictation::Recording> {
        self.underway.as_ref().map(|u| &u.recording).or_else(|| self.space_listening())
    }
}
