//! Plain Space first types, listens after 0.2 s, then dictates after 0.5 s. A short press or any other key cancels
//! that preparation. The recording and timer belong to the field that received the press, not wherever focus goes.

use std::time::Duration;

use dictation::{Recording, Update};
use gpui_kit::component::input::Rope;
use gpui_kit::{Context, Focusable as _, KeyDownEvent, Keystroke, Task, Window};
use sound::Permission;

use crate::recording::Started;
use crate::text::Before;
use crate::{Field, keys, source};

/// A space being typed is released before this, so ordinary typing never opens the microphone.
pub const SPACE_LISTENS_AFTER: Duration = Duration::from_millis(200);
/// Long enough to distinguish a held key, independent of the system's key-repeat setting.
pub const SPACE_DICTATES_AFTER: Duration = Duration::from_millis(500);

pub(crate) struct SpaceDown {
    before: Before,
    typed: Option<Rope>,
    listening: Option<(Recording, flume::Receiver<Update>)>,
    _timing: Task<()>,
}

impl Field {
    pub(crate) fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !plain_space(&event.keystroke) {
            return;
        }
        if !event.is_held {
            self.space_down = None;
            if self.input.focus_handle(cx).is_focused(window) && self.underway.is_none() && self.can_dictate(cx) {
                source::claim(cx);
                let before = Before::of(self.input.read(cx));
                let timing = cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(SPACE_LISTENS_AFTER).await;
                    if this.update(cx, Field::listen).is_err() {
                        return;
                    }
                    cx.background_executor().timer(SPACE_DICTATES_AFTER.saturating_sub(SPACE_LISTENS_AFTER)).await;
                    let _ = this.update_in(cx, Self::held_long_enough);
                });
                self.space_down = Some(SpaceDown { before, typed: None, listening: None, _timing: timing });
            }
        } else if self.space_down.is_some() {
            cx.stop_propagation();
        }
    }

    pub(crate) fn key_pressed(&mut self, keystroke: &Keystroke) {
        if !plain_space(keystroke) {
            self.space_down = None;
        }
    }

    fn listen(&mut self, cx: &mut Context<Self>) {
        let hints = self.current_hints(cx);
        let Some(down) = self.space_down.as_mut() else { return };
        down.typed = Some(self.input.read(cx).text().clone());
        if !source::uses_microphone(cx) || sound::microphone_permission() == Permission::Allowed {
            down.listening = source::record(hints, cx);
        }
    }

    fn held_long_enough(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(down) = self.space_down.take() else { return };
        if !self.can_dictate(cx) || !self.input.focus_handle(cx).is_focused(window) {
            return;
        }
        keys::held(cx);
        if down.typed.as_ref() == Some(self.input.read(cx).text()) {
            self.put_back(&down.before, window, cx);
        }
        match down.listening {
            Some((recording, updates)) => self.start_with(recording, updates, Started::Key, window, cx),
            None => self.start(Started::Key, window, cx),
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn space_listening(&self) -> Option<&Recording> {
        self.space_down.as_ref()?.listening.as_ref().map(|(recording, _)| recording)
    }
}

fn plain_space(keystroke: &Keystroke) -> bool {
    keystroke.key == keys::SPACE && !keystroke.modifiers.modified()
}
