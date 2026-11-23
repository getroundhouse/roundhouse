//! What a test has the system say of the app's permissions, in place of asking it. Every permission is unknown until
//! the test says how it stands, and what the app asks of the system is noted for the test to read.

use std::collections::HashMap;

use gpui_kit::{App, Global, Task};

use super::super::{Badges, Permission, Standing};

#[derive(Default)]
struct System {
    standings: HashMap<Permission, Standing>,
    asked: Vec<Permission>,
    opened: Vec<Permission>,
}

impl Global for System {}

fn system(cx: &mut App) -> &mut System {
    cx.default_global::<System>()
}

/// Has the system say `permission` stands as `standing`, from the next read on.
pub fn set_standing(permission: Permission, standing: Standing, cx: &mut App) {
    system(cx).standings.insert(permission, standing);
}

/// Has the app read every permission again, as it does when its window comes in front.
pub fn read_again(cx: &mut App) {
    crate::permissions::read(cx);
}

/// The permissions the app has had the system ask the person for, in order.
pub fn asked_for(cx: &App) -> Vec<Permission> {
    cx.try_global::<System>().map_or_else(Vec::new, |system| system.asked.clone())
}

/// The permissions whose place in System Settings the app has opened, in order.
pub fn opened_for(cx: &App) -> Vec<Permission> {
    cx.try_global::<System>().map_or_else(Vec::new, |system| system.opened.clone())
}

pub(in crate::permissions) fn read(cx: &mut App) -> Task<HashMap<Permission, Standing>> {
    let set = &system(cx).standings;
    let read = Permission::ALL.into_iter().map(|p| (p, set.get(&p).copied().unwrap_or(Standing::Unknown))).collect();
    Task::ready(read)
}

pub(in crate::permissions) fn ask(permission: Permission, cx: &mut App) -> Task<()> {
    system(cx).asked.push(permission);
    Task::ready(())
}

pub(in crate::permissions) fn open_settings(permission: Permission, cx: &mut App) {
    system(cx).opened.push(permission);
}

/// Nothing, since a test has no Dock.
pub(in crate::permissions) fn ask_for_badges(_: &mut App) -> Task<Badges> {
    Task::ready(Badges::Settled)
}
