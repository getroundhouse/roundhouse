//! Whether Space is down, by the system's word rather than by the keys the app has heard, since letting go of it while
//! another app is in front, even for a moment, tells only that app. A Mac says so only to an app the person has let
//! monitor input, which the app asks once something has been dictated. A test says itself what the system would.

use gpui_kit::App;

/// Whether Space is down now, or `None` where the system won't say, as a Mac won't to an app not let monitor input.
#[cfg(not(feature = "testing"))]
pub(crate) fn space_is_down(_: &App) -> Option<bool> {
    system::space_is_down()
}

/// Has the system ask the person to let the app monitor input, unless it may already. A Mac asks only the first time.
#[cfg(not(feature = "testing"))]
pub(crate) fn ask_to_see_the_keyboard(_: &mut App) {
    system::ask_to_see_the_keyboard();
}

/// Whether Space is down now, as the test has said with [`hold_space_down`] or [`hide_the_keyboard`]. It's up until the
/// test says otherwise.
#[cfg(feature = "testing")]
pub(crate) fn space_is_down(cx: &App) -> Option<bool> {
    cx.try_global::<Keyboard>().map_or(Some(false), |keyboard| keyboard.space)
}

/// Notes that the system would have been asked, for [`asked_to_see_the_keyboard`].
#[cfg(feature = "testing")]
pub(crate) fn ask_to_see_the_keyboard(cx: &mut App) {
    keyboard(cx).asked = true;
}

/// What the test has the system say of the keyboard, which the app asks the system in place of this.
#[cfg(feature = "testing")]
struct Keyboard {
    space: Option<bool>,
    asked: bool,
}

#[cfg(feature = "testing")]
impl gpui_kit::Global for Keyboard {}

#[cfg(feature = "testing")]
fn keyboard(cx: &mut App) -> &mut Keyboard {
    if !cx.has_global::<Keyboard>() {
        cx.set_global(Keyboard { space: Some(false), asked: false });
    }
    cx.global_mut::<Keyboard>()
}

/// Has Space be down, or up, as the system would say, whichever keys the app has been given.
#[cfg(feature = "testing")]
pub fn hold_space_down(down: bool, cx: &mut App) {
    keyboard(cx).space = Some(down);
}

/// Has the system not say whether Space is down, as a Mac won't to an app not let monitor input.
#[cfg(feature = "testing")]
pub fn hide_the_keyboard(cx: &mut App) {
    keyboard(cx).space = None;
}

/// Whether the app has had the system ask to let it monitor input.
#[cfg(feature = "testing")]
pub fn asked_to_see_the_keyboard(cx: &App) -> bool {
    cx.try_global::<Keyboard>().is_some_and(|keyboard| keyboard.asked)
}

#[cfg(all(target_os = "macos", not(feature = "testing")))]
mod system {
    use objc2_core_graphics::{
        CGEventSource, CGEventSourceStateID, CGKeyCode, CGPreflightListenEventAccess, CGRequestListenEventAccess,
    };

    /// `kVK_Space`, which is Space whatever the keyboard's layout.
    const SPACE: CGKeyCode = 0x31;

    /// What the keyboard has down, rather than what the events posted since have made of it. Without leave to monitor
    /// input, a Mac says no key is down while another app is in front, so that's taken as not knowing.
    pub(super) fn space_is_down() -> Option<bool> {
        CGPreflightListenEventAccess().then(|| CGEventSource::key_state(CGEventSourceStateID::HIDSystemState, SPACE))
    }

    pub(super) fn ask_to_see_the_keyboard() {
        if !CGPreflightListenEventAccess() {
            // Whether it may comes later, from System Settings, so what this says now isn't needed.
            let _ = CGRequestListenEventAccess();
        }
    }
}

/// Where nothing is dictated, so Space is never held to dictate and nothing needs asking.
#[cfg(all(not(target_os = "macos"), not(feature = "testing")))]
mod system {
    /// Nothing here asks the system.
    pub(super) fn space_is_down() -> Option<bool> {
        None
    }

    pub(super) fn ask_to_see_the_keyboard() {}
}
