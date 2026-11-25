//! The system the app was given to ask.

use std::collections::HashMap;

use gpui_kit::{App, Global, Task};

#[cfg(target_os = "macos")]
use super::mac as machines;
#[cfg(not(target_os = "macos"))]
use super::none as machines;
use super::none;
use crate::permissions::{Badges, Permission, Standing};

/// The system the app asks.
#[derive(Clone, Copy)]
pub(crate) struct System {
    pub(super) read: fn(&mut App) -> Task<HashMap<Permission, Standing>>,
    pub(super) ask: fn(Permission, &mut App) -> Task<()>,
    pub(super) open_settings: fn(Permission, &mut App),
    pub(super) ask_for_badges: fn(&mut App) -> Task<Badges>,
}

impl Global for System {}

impl System {
    /// The one this machine runs.
    pub(crate) const THIS_MACHINES: Self = Self {
        read: machines::read,
        ask: machines::ask,
        open_settings: machines::open_settings,
        ask_for_badges: machines::ask_for_badges,
    };

    /// None, which says nothing of any permission.
    const NONE: Self = Self {
        read: none::read,
        ask: none::ask,
        open_settings: none::open_settings,
        ask_for_badges: none::ask_for_badges,
    };

    /// The one the app was given.
    fn of(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or(Self::NONE)
    }
}

/// How every permission stands.
pub(in crate::permissions) fn read(cx: &mut App) -> Task<HashMap<Permission, Standing>> {
    (System::of(cx).read)(cx)
}

/// Has the system ask the person for `permission`, until they've answered.
pub(in crate::permissions) fn ask(permission: Permission, cx: &mut App) -> Task<()> {
    (System::of(cx).ask)(permission, cx)
}

/// Opens the place in System Settings where `permission` is switched on.
pub(in crate::permissions) fn open_settings(permission: Permission, cx: &mut App) {
    (System::of(cx).open_settings)(permission, cx);
}

/// Asks for the Dock's badge along with notifications.
pub(in crate::permissions) fn ask_for_badges(cx: &mut App) -> Task<Badges> {
    (System::of(cx).ask_for_badges)(cx)
}
