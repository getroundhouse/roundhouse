//! The release of a press, given to the window when the system kept it. On Windows, a press on the window's frame,
//! to resize the window, or on what Windows takes for its title bar, to move it, reaches the app first, and unless the
//! app stops it, Windows then runs a loop of its own that takes the release. GPUI makes none up once the loop ends, so
//! whatever waits for a release went on waiting, and the kit's selection followed the pointer over a chat's text, with
//! no button held, until the next click.
//!
//! So once a left press has had no release, a move that says the left button isn't held asks the system whether it's
//! down. If it isn't, the release was lost, and the window is given one where the pointer is, before anything hears
//! the move. The move's word isn't enough, because GPUI on Windows says no button is held on every move over the
//! frame or a title bar, even while one is, as it is through a press there that the app kept.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    App, DispatchPhase, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput,
    Styled as _, Window, canvas,
};

/// Whether the window has had a left press and no release since. The window's view keeps it, so it holds from one
/// frame to the next, while what [`Unreleased::hears`] gives is drawn again on each.
#[derive(Clone, Default)]
pub(crate) struct Unreleased(Rc<Cell<bool>>);

impl Unreleased {
    /// Hears every press, release and move in the window, wherever the pointer is. The window's view draws it before
    /// the rest of what it draws, so it hears each one first.
    pub(crate) fn hears(&self) -> impl IntoElement {
        let unreleased = self.clone();
        canvas(|_, _, _| {}, move |_, (), window, _| unreleased.listen(window)).absolute()
    }

    fn listen(&self, window: &mut Window) {
        let pressed = self.0.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, _, _| {
            if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                pressed.set(true);
            }
        });
        let released = self.0.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
            if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                released.set(false);
            }
        });
        let unreleased = self.0.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture
                || !unreleased.get()
                || event.pressed_button == Some(MouseButton::Left)
                || left_button(cx) == Button::Down
            {
                return;
            }
            unreleased.set(false);
            // The release goes first, so nothing follows the pointer as if the button were still down, and then the
            // move, for whatever is under the pointer. Neither can be dispatched from in here, since GPUI holds the
            // window's listeners while it dispatches, so both go once this dispatch is done, before the system's next
            // event.
            cx.stop_propagation();
            let moved = event.clone();
            window.defer(cx, move |window, cx| {
                let (position, modifiers) = (moved.position, moved.modifiers);
                let up = MouseUpEvent { button: MouseButton::Left, position, modifiers, click_count: 1 };
                window.dispatch_event(PlatformInput::MouseUp(up), cx);
                window.dispatch_event(PlatformInput::MouseMove(moved), cx);
            });
        });
    }
}

/// Where a mouse button is, as the system says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Down,
    Up,
}

/// Where the left button is now, as the system says, or as a test says in its place.
#[cfg(feature = "testing")]
fn left_button(cx: &App) -> Button {
    cx.try_global::<StandIn>().map_or_else(system::left_button, |StandIn(button)| *button)
}

#[cfg(not(feature = "testing"))]
fn left_button(_: &App) -> Button {
    system::left_button()
}

/// What a test says of the left button, in place of the system.
#[cfg(feature = "testing")]
struct StandIn(Button);

#[cfg(feature = "testing")]
impl gpui_kit::Global for StandIn {}

/// Says the left button is `button` from now on, whatever the system would say. Only Windows is asked, and a test
/// can't run there, or hold its button.
#[cfg(feature = "testing")]
pub fn the_left_button_is(button: Button, cx: &mut App) {
    cx.set_global(StandIn(button));
}

#[cfg(target_os = "windows")]
mod system {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};

    use super::Button;

    /// As it is now, rather than as of the last message the app read, since a release that went to another window,
    /// or to Windows' own loop, is never read here. `GetAsyncKeyState` reads the buttons as they're built, so with
    /// them swapped in Settings, the one Windows calls left is the one on the right.
    pub(super) fn left_button() -> Button {
        // SAFETY: it takes no pointer, and any index.
        let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
        let built = if swapped { VK_RBUTTON } else { VK_LBUTTON };
        // SAFETY: it takes no pointer, and any key.
        let state = unsafe { GetAsyncKeyState(i32::from(built)) };
        // The high bit says it's down.
        if state < 0 { Button::Down } else { Button::Up }
    }
}

#[cfg(not(target_os = "windows"))]
mod system {
    use super::Button;

    /// It's only asked when a move has said the button isn't held, which elsewhere is so.
    pub(super) fn left_button() -> Button {
        Button::Up
    }
}
