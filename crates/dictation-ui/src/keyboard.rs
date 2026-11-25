//! Whether Space is down, by the system's word rather than by the keys the app has heard, since letting go of it while
//! another app is in front, even for a moment, tells only that app. A Mac says so only to an app the person has let
//! monitor input, which the app asks once something has been dictated. The app as it runs watches the system's keyboard,
//! and a test says itself what the system would.

use gpui_kit::{App, Global};

/// What says how the keyboard is.
#[derive(Clone, Copy)]
struct Keyboard {
    space_is_down: fn(&App) -> Option<bool>,
    ask_to_see: fn(&mut App),
}

impl Global for Keyboard {}

impl Keyboard {
    /// None, which won't say, and has nobody to ask.
    const NONE: Self = Self { space_is_down: |_| None, ask_to_see: |_| {} };

    /// The one the app was given.
    fn of(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or(Self::NONE)
    }
}

/// Has fields go by the system's keyboard, as the app does as it runs. Without this, nothing asks the system.
pub fn watch_keyboard(cx: &mut App) {
    cx.set_global(Keyboard {
        space_is_down: |_| system::space_is_down(),
        ask_to_see: |_| system::ask_to_monitor_input(),
    });
}

/// Whether Space is down now, or `None` where the system won't say, as a Mac won't to an app not let monitor input.
pub(crate) fn space_is_down(cx: &App) -> Option<bool> {
    (Keyboard::of(cx).space_is_down)(cx)
}

/// Has the system ask the person to let the app monitor input, unless it may already. A Mac asks only the first time.
pub(crate) fn ask_to_see_the_keyboard(cx: &mut App) {
    (Keyboard::of(cx).ask_to_see)(cx);
}

/// What a test has the system say of the keyboard, and whether it would have been asked.
#[cfg(any(test, feature = "testing"))]
struct Said {
    space: Option<bool>,
    asked: bool,
}

#[cfg(any(test, feature = "testing"))]
impl Global for Said {}

/// What the test has the system say, which fields go by from the first time this is called. Space is up until the test
/// says otherwise.
#[cfg(any(test, feature = "testing"))]
fn said(cx: &mut App) -> &mut Said {
    if !cx.has_global::<Said>() {
        cx.set_global(Said { space: Some(false), asked: false });
        cx.set_global(Keyboard {
            space_is_down: |cx| cx.try_global::<Said>().and_then(|said| said.space),
            ask_to_see: |cx| said(cx).asked = true,
        });
    }
    cx.global_mut::<Said>()
}

/// Has Space be down, or up, as the system would say, whichever keys the app has been given.
#[cfg(any(test, feature = "testing"))]
pub fn hold_space_down(down: bool, cx: &mut App) {
    said(cx).space = Some(down);
}

/// Has the system not say whether Space is down, as a Mac won't to an app not let monitor input.
#[cfg(any(test, feature = "testing"))]
pub fn hide_the_keyboard(cx: &mut App) {
    said(cx).space = None;
}

/// Whether the app has had the system ask to let it monitor input.
#[cfg(any(test, feature = "testing"))]
pub fn asked_to_see_the_keyboard(cx: &App) -> bool {
    cx.try_global::<Said>().is_some_and(|said| said.asked)
}

/// What a Mac says of the keyboard. Settings' Permissions page asks for leave to monitor input through it too, so the
/// app has one way to.
#[cfg(target_os = "macos")]
pub(crate) mod system {
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

    /// Has the system ask the person to let the app monitor input, unless it may already. A Mac asks only the first
    /// time, and lists the app in System Settings, switched off, for the person to switch on.
    pub fn ask_to_monitor_input() {
        if !CGPreflightListenEventAccess() {
            // Whether it may comes later, from System Settings, so what this says now isn't needed.
            let _ = CGRequestListenEventAccess();
        }
    }
}

/// What a Mac says of the app monitoring input.
#[cfg(target_os = "macos")]
pub(crate) mod monitoring {
    use objc2_io_kit::{IOHIDAccessType, IOHIDCheckAccess, IOHIDRequestType};
    use sound::Permission;

    /// What the person has said about the app monitoring input. `IOKit`, unlike Core Graphics, says whether they've
    /// been asked at all.
    pub fn input_monitoring() -> Permission {
        match IOHIDCheckAccess(IOHIDRequestType::ListenEvent) {
            IOHIDAccessType::Granted => Permission::Allowed,
            IOHIDAccessType::Unknown => Permission::NotAsked,
            _ => Permission::Denied,
        }
    }
}

/// Where nothing is dictated, so Space is never held to dictate and nothing needs asking.
#[cfg(not(target_os = "macos"))]
mod system {
    /// Nothing here asks the system.
    pub(super) fn space_is_down() -> Option<bool> {
        None
    }

    pub(super) fn ask_to_monitor_input() {}
}
