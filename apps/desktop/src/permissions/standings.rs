//! How each permission stood when the app last read them, which the Permissions page draws. They're read as the app
//! opens and each time its window comes in front, which is when the person may have changed one in System Settings,
//! and again once the system has answered what the page asked.

use std::collections::HashMap;

use gpui_kit::{App, Global};

use super::system;
use super::{Badges, Permission, Standing};

/// How each permission stood at the last read.
#[derive(Default)]
struct Standings(HashMap<Permission, Standing>);

impl Global for Standings {}

/// How `permission` stood at the last read, or `None` before the first has finished.
pub(crate) fn standing(permission: Permission, cx: &App) -> Option<Standing> {
    cx.try_global::<Standings>().and_then(|standings| standings.0.get(&permission).copied())
}

/// Reads how every permission stands, off the main thread, and redraws once it knows.
pub(crate) fn read(cx: &mut App) {
    if !super::HAS_PERMISSIONS {
        return;
    }
    let reading = system::read(cx);
    cx.spawn(async move |cx| {
        let read = reading.await;
        cx.update(|cx| {
            cx.set_global(Standings(read));
            cx.refresh_windows();
        });
    })
    .detach();
}

/// Does what fixes `permission`, as its row's button says: has the system ask the person, if it hasn't, or opens
/// System Settings where it's switched on, having first had the app listed there, then reads it again. Nothing for
/// one that's allowed, or that the system won't say of.
pub(crate) fn fix(permission: Permission, cx: &mut App) {
    match standing(permission, cx) {
        Some(Standing::NotAsked) => {
            let answered = system::ask(permission, cx);
            cx.spawn(async move |cx| {
                answered.await;
                cx.update(read);
            })
            .detach();
        }
        Some(Standing::Off) => system::open_settings(permission, cx),
        Some(Standing::Allowed | Standing::Unknown) | None => {}
    }
}

/// Asks for the Dock's badge along with notifications, if they're allowed without it, which a Mac grants without
/// asking the person, then calls `answered` with where that leaves it, so the badge can be shown again once granted.
pub(crate) fn ask_for_badges(cx: &mut App, answered: impl FnOnce(Badges, &mut App) + 'static) {
    let asking = system::ask_for_badges(cx);
    cx.spawn(async move |cx| {
        let badges = asking.await;
        cx.update(|cx| answered(badges, cx));
    })
    .detach();
}
