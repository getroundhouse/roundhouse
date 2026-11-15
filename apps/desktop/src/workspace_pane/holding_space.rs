//! Holding plain Space in the composer to dictate, as Claude Code's voice mode does, beside `Option-Space`, which
//! starts at once. Space types as it always has, and what it does while held is timed from the press: from
//! [`SPACE_LISTENS_AFTER`], which a space being typed never lasts, it hears what's said without showing it, and from
//! [`SPACE_DICTATES_AFTER`] it takes back out the space it typed and dictates, as `Option-Space` does, with what was said
//! since it began to listen. So the person can start talking as they press, rather than wait to be told to. Letting go,
//! or another key, before then leaves the space, and what was heard goes unseen. Its repeats, which the system sends
//! while it's held, type nothing meanwhile, so the timing doesn't follow the key repeat, and Space dictates with key
//! repeat off. *Unconfirmed.*

use std::time::Duration;

use dictation::{Permission, Recording, Update};
use gpui_kit::component::input::Rope;
use gpui_kit::{Context, Focusable as _, KeyDownEvent, Keystroke, Task, Window};
use protocol::model::SessionId;

use super::WorkspacePane;
use super::dictating::{Before, Started};
use crate::dictating::{self, SPACE};

/// How long Space is held, from the press, before what's said is heard, without showing it: longer than a space being
/// typed is held, so the microphone isn't opened as someone types. *Unconfirmed.*
pub const SPACE_LISTENS_AFTER: Duration = Duration::from_millis(200);

/// How long Space is held, from the press, before it dictates, showing what's been heard since it began to listen.
/// *Unconfirmed.*
pub const SPACE_DICTATES_AFTER: Duration = Duration::from_millis(500);

/// Space, pressed where a prompt is typed, with dictation ready and nothing being dictated, and not let go of since.
/// Dropping it forgets it, which cancels what it was hearing.
pub(super) struct SpaceDown {
    /// The chat whose composer it was pressed in.
    chat: SessionId,
    /// What the composer had before Space typed, which goes back once it dictates.
    before: Before,
    /// What it had once Space began to listen, once the space had typed. If something else changes it from then until
    /// it dictates, such as a paste from the Edit menu, it's left as it is, the space too.
    typed: Option<Rope>,
    /// What's been heard, unseen, since it began to listen: a recording and what's heard of it, waiting to be shown.
    /// There's none before then, and none while the system hasn't yet been asked whether the app may use the
    /// microphone, since asking shows a dialog, which waits until Space dictates.
    listening: Option<(Recording, flume::Receiver<Update>)>,
    /// Times listening and then dictating from the press. It goes with the press.
    _timing: Task<()>,
}

impl WorkspacePane {
    /// Space going down in the composer, before anything there hears it, or it types. Other keys are heard once they've
    /// gone where they go, by [`Self::key_pressed`].
    pub(super) fn composer_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !plain_space(&event.keystroke) {
            return;
        }
        if !event.is_held {
            self.forget_space_down();
            self.space_down = self.space_could_dictate(window, cx).map(|chat| {
                let before = Before::of(self.composer.read(cx));
                SpaceDown { chat, before, typed: None, listening: None, _timing: Self::time_space(window, cx) }
            });
        } else if self.space_down.is_some() {
            // A repeat while it's held towards dictating types nothing.
            cx.stop_propagation();
        }
    }

    /// Listens, and then dictates, as long as Space is held.
    fn time_space(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SPACE_LISTENS_AFTER).await;
            if this.update_in(cx, |t, _, cx| t.space_listens(cx)).is_err() {
                return;
            }
            cx.background_executor().timer(SPACE_DICTATES_AFTER.saturating_sub(SPACE_LISTENS_AFTER)).await;
            let _ = this.update_in(cx, Self::space_dictates);
        })
    }

    /// A key pressed in any window, once a binding or the text has taken it, or nothing has. Any but plain Space means
    /// the person is typing rather than holding Space to dictate. A key a binding takes, such as Enter or Backspace,
    /// never reaches [`Self::composer_key_down`], so it's heard here.
    pub(super) fn key_pressed(&mut self, keystroke: &Keystroke) {
        if !plain_space(keystroke) {
            self.forget_space_down();
        }
    }

    /// The chat Space could dictate into once held: the one whose composer is drawn, while dictation is ready, nothing
    /// is being dictated, and Space was pressed where the prompt is typed, rather than in the bar or its menus.
    fn space_could_dictate(&self, window: &Window, cx: &Context<Self>) -> Option<SessionId> {
        let typing = self.composer.focus_handle(cx).is_focused(window);
        let can = typing && self.dictation.is_none() && self.can_dictate(cx);
        self.active.clone().filter(|_| can)
    }

    /// Space has been held long enough to begin to listen, unseen, once the system allows the microphone.
    fn space_listens(&mut self, cx: &mut Context<Self>) {
        let Some(down) = self.space_down.as_mut() else { return };
        down.typed = Some(self.composer.read(cx).text().clone());
        let allowed = !dictating::hears_the_microphone(cx) || dictation::microphone_permission() == Permission::Allowed;
        if allowed {
            down.listening = dictating::record(cx);
        }
    }

    /// Space has been held long enough to dictate, with what it's heard since it began to listen.
    fn space_dictates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(down) = self.space_down.take() else { return };
        // Space's repeats type nothing more until it's let go of, even if dictating doesn't start, as when the
        // system asks first whether the app may use the microphone.
        dictating::held(cx);
        if down.typed.as_ref() == Some(self.composer.read(cx).text()) {
            self.put_back(&down.before, window, cx);
        }
        match down.listening {
            Some((recording, updates)) => self.dictate_with(recording, updates, Started::ByTheKey, window, cx),
            None => self.dictate(Started::ByTheKey, window, cx),
        }
    }

    /// Space was let go of, another key pressed, something started dictating, or the composer it was pressed in has
    /// gone, so holding it won't dictate, what it typed stays, and what it heard goes unseen.
    pub(super) fn forget_space_down(&mut self) {
        self.space_down = None;
    }

    /// Forgets Space held in a composer that isn't drawn any more.
    pub(super) fn space_down_follows_its_composer(&mut self, drawn: Option<&SessionId>) {
        if self.space_down.as_ref().is_some_and(|down| drawn != Some(&down.chat)) {
            self.forget_space_down();
        }
    }

    /// The recording Space held towards dictating is hearing, unseen, for a test to give it audio.
    #[cfg(feature = "testing")]
    pub(super) fn space_listening(&self) -> Option<&Recording> {
        self.space_down.as_ref()?.listening.as_ref().map(|(recording, _)| recording)
    }
}

fn plain_space(keystroke: &Keystroke) -> bool {
    keystroke.key == SPACE && !keystroke.modifiers.modified()
}
