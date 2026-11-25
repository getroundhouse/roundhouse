//! What of the window the system draws over, such as the status bar and the home indicator, or the keyboard covers,
//! which every screen keeps what it shows clear of.

use gpui_kit::{App, Window, WindowInsets};

/// A phone's safe area, given to a test's window, which has none of its own. GPUI's test window can be given
/// one, but only by GPUI's own tests, since what reaches it is private to GPUI. Every screen reads it from here, so this
/// is all that stands in for the window's.
#[cfg(feature = "testing")]
struct Given(gpui_kit::Edges<gpui_kit::Pixels>);

#[cfg(feature = "testing")]
impl gpui_kit::Global for Given {}

/// Has every window drawn from now on keep clear of `safe_area`, as a phone's does of its status bar and home
/// indicator, for a picture of a screen as a phone shows it.
#[cfg(feature = "testing")]
pub fn the_safe_area_is(safe_area: gpui_kit::Edges<gpui_kit::Pixels>, cx: &mut App) {
    cx.set_global(Given(safe_area));
}

/// What of `window` is under the system's own parts, and the keyboard.
#[cfg(feature = "testing")]
pub(crate) fn insets(window: &Window, cx: &App) -> WindowInsets {
    let insets = window.insets();
    match cx.try_global::<Given>() {
        Some(Given(safe_area)) => WindowInsets { safe_area: *safe_area, ..insets },
        None => insets,
    }
}

/// What of `window` is under the system's own parts, and the keyboard.
#[cfg(not(feature = "testing"))]
pub(crate) fn insets(window: &Window, _: &App) -> WindowInsets {
    window.insets()
}
