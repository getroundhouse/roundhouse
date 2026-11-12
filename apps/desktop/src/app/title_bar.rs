//! The window has no title bar, so the strip along its top, as tall as the workspace header, stands in for one:
//! dragging it moves the window, and double-clicking it does what double-clicking a title bar does, which on a Mac is
//! what its setting says, and on Windows maximizes or restores the window. A press there that a control took, such as
//! a button in the header, is the control's. On Windows it also holds the window's buttons.

use gpui_kit::{
    App, Context, Div, InteractiveElement as _, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _,
    Stateful, Styled as _, Window, WindowControlArea, div,
};

use super::AppView;
use crate::sides::WindowButtons;
use crate::workspace_pane::HEADER_HEIGHT;

/// Who moves the window from the strip, which goes by the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleBar {
    /// The app, once a press on the strip moves, and it asks the system what a double click does. On a Mac and Linux.
    App,
    /// Windows, from its own hit test, which takes the strip for a title bar, and it maximizes or restores the window
    /// on a double click. GPUI's Windows backend can't start a move or answer a double click for the app. The app draws
    /// Windows' buttons at the strip's right end, which GPUI answers the same hit test with.
    Windows,
}

impl TitleBar {
    const OF_THE_SYSTEM: Self = if cfg!(target_os = "windows") { Self::Windows } else { Self::App };
}

/// Where the window's buttons are in the frame the window is starting to draw. In full screen the system puts them
/// away.
pub(super) fn window_buttons(window: &Window, cx: &App) -> WindowButtons {
    match title_bar(cx) {
        _ if window.is_fullscreen() => WindowButtons::Absent,
        TitleBar::Windows => WindowButtons::AtTheEnd,
        TitleBar::App if cfg!(target_os = "macos") => WindowButtons::InTheCorner,
        TitleBar::App => WindowButtons::Absent,
    }
}

/// The window's, as the system has it, or as a test says in its place.
#[cfg(feature = "testing")]
fn title_bar(cx: &App) -> TitleBar {
    cx.try_global::<StandIn>().map_or(TitleBar::OF_THE_SYSTEM, |StandIn(title_bar)| *title_bar)
}

#[cfg(not(feature = "testing"))]
fn title_bar(_: &App) -> TitleBar {
    TitleBar::OF_THE_SYSTEM
}

/// What a test says of the title bar, in place of the system.
#[cfg(feature = "testing")]
struct StandIn(TitleBar);

#[cfg(feature = "testing")]
impl gpui_kit::Global for StandIn {}

/// Says the window's title bar is `title_bar` from the next frame on, whatever the system has, so a test can stand in
/// for Windows, where the tests mostly don't build.
#[cfg(feature = "testing")]
pub fn the_title_bar_is(title_bar: TitleBar, cx: &mut App) {
    cx.set_global(StandIn(title_bar));
}

/// `root`, the whole window, with the strip along its top. It goes in before what the window shows, so it's under
/// all of it, and what hides what's under it from the pointer hides the strip too: the kit's resize handle on a
/// sidebar's edge, a dialog's backdrop, and Home's search.
pub(super) fn stands_in(root: Stateful<Div>, cx: &mut Context<AppView>) -> Stateful<Div> {
    let title_bar = title_bar(cx);
    let strip = div().absolute().top_0().left_0().w_full().h(HEADER_HEIGHT).on_mouse_down(
        MouseButton::Left,
        cx.listener(move |t, event: &MouseDownEvent, window, cx| t.pressed_the_strip(title_bar, event, window, cx)),
    );
    match title_bar {
        // GPUI answers Windows' hit test as a title bar over what's marked so, and nowhere that hides it. A press
        // there is then one on the window's frame, which GPUI doesn't capture the mouse for, and while the pointer is
        // over the strip it says no button is held on each move. So what follows a held pointer in the strip, as a
        // text field does as it selects, hides the strip, as Home's search does.
        TitleBar::Windows => root.child(strip.window_control_area(WindowControlArea::Drag)),
        // Moves anywhere in the window, since the first can already be off the strip.
        TitleBar::App => root
            .child(strip)
            .on_mouse_move(cx.listener(|t, event: &MouseMoveEvent, window, _| {
                if std::mem::take(&mut t.moving_window) && event.pressed_button == Some(MouseButton::Left) {
                    window.start_window_move();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|t, _, _, _| t.moving_window = false)),
    }
}

impl AppView {
    /// A left press on the strip, which hears it after everything over it. One of the kit's buttons, or anything that
    /// takes focus, marks a press as taken, and a greyed-out button stops it, so it never gets here.
    fn pressed_the_strip(&mut self, title_bar: TitleBar, event: &MouseDownEvent, window: &mut Window, cx: &mut App) {
        if window.default_prevented() {
            // GPUI hands Windows a press on the strip once the app is done with it, unless it's stopped, and Windows
            // would take it for its title bar's and move the window.
            if title_bar == TitleBar::Windows {
                cx.stop_propagation();
            }
            return;
        }
        // Pressing a title bar leaves focus where it was.
        window.prevent_default();
        match title_bar {
            // Windows moves the window, once the press goes on to it.
            TitleBar::Windows => {}
            TitleBar::App => {
                self.moving_window = event.click_count != 2;
                if event.click_count == 2 {
                    window.titlebar_double_click();
                }
            }
        }
    }
}
