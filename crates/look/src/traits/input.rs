use gpui_kit::{App, Global};

/// How what's on screen is pressed: with a pointer, which can rest on something before it presses, or by touch, which
/// can't, and is a finger rather than a point. A component responds as its input asks, so no screen does: under touch,
/// a button has no tooltip, since nothing rests on it. It's the app's, and a pointer until the app says otherwise, since
/// it can change while the app runs, as a trackpad is attached to a tablet. *Unconfirmed.* *Not built:* a tap over at
/// least 44 points, as iOS asks, whatever size a button is drawn, so what's tapped is drawn that large instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Input {
    #[default]
    Pointer,
    Touch,
}

impl Global for Input {}

impl Input {
    /// How `cx` is pressed now.
    pub fn of(cx: &App) -> Input {
        cx.try_global::<Input>().copied().unwrap_or_default()
    }

    /// Has everything drawn from now on respond to this input. The app's windows are drawn again with it.
    pub fn choose(self, cx: &mut App) {
        cx.set_global(self);
        cx.refresh_windows();
    }
}
