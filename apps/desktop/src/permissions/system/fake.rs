//! What a test has the system say of the app's permissions, in place of asking it. Every permission is unknown until
//! the test says how it stands, and what the app asks of the system is noted for the test to read.

use std::collections::HashMap;

use gpui_kit::{App, Global, Task};

use super::System;
use crate::permissions::{Badges, Permission, Standing};

#[derive(Default)]
struct Said {
    standings: HashMap<Permission, Standing>,
    asked: Vec<Permission>,
    opened: Vec<Permission>,
}

impl Global for Said {}

/// What the test has the system say, which the app asks in place of any other from the first time this is called.
fn said(cx: &mut App) -> &mut Said {
    if !cx.has_global::<Said>() {
        cx.set_global(System { read, ask, open_settings, ask_for_badges });
    }
    cx.default_global::<Said>()
}

/// Has the system say `permission` stands as `standing`, from the next read on.
pub fn set_standing(permission: Permission, standing: Standing, cx: &mut App) {
    said(cx).standings.insert(permission, standing);
}

/// Has the app read every permission again, as it does when its window comes in front.
pub fn read_again(cx: &mut App) {
    crate::permissions::read(cx);
}

/// The permissions the app has had the system ask the person for, in order.
pub fn asked_for(cx: &App) -> Vec<Permission> {
    cx.try_global::<Said>().map_or_else(Vec::new, |said| said.asked.clone())
}

/// The permissions whose place in System Settings the app has opened, in order.
pub fn opened_for(cx: &App) -> Vec<Permission> {
    cx.try_global::<Said>().map_or_else(Vec::new, |said| said.opened.clone())
}

fn read(cx: &mut App) -> Task<HashMap<Permission, Standing>> {
    let set = &said(cx).standings;
    let read = Permission::ALL.into_iter().map(|p| (p, set.get(&p).copied().unwrap_or(Standing::Unknown))).collect();
    Task::ready(read)
}

fn ask(permission: Permission, cx: &mut App) -> Task<()> {
    said(cx).asked.push(permission);
    Task::ready(())
}

fn open_settings(permission: Permission, cx: &mut App) {
    said(cx).opened.push(permission);
}

/// Nothing, since a test has no Dock.
fn ask_for_badges(_: &mut App) -> Task<Badges> {
    Task::ready(Badges::Settled)
}
