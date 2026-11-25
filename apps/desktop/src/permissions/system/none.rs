//! No system to ask, as on Windows and Linux, which show no Permissions page, as `HAS_PERMISSIONS` says, and in an app
//! given none.

use std::collections::HashMap;

use gpui_kit::{App, Task};

use super::super::{Badges, Permission, Standing};

/// Every permission unknown.
pub(in crate::permissions) fn read(_: &mut App) -> Task<HashMap<Permission, Standing>> {
    Task::ready(Permission::ALL.into_iter().map(|permission| (permission, Standing::Unknown)).collect())
}

/// Nothing, since there's nothing to ask.
pub(in crate::permissions) fn ask(_: Permission, _: &mut App) -> Task<()> {
    Task::ready(())
}

/// Nothing, since there's nothing to open.
pub(in crate::permissions) fn open_settings(_: Permission, _: &mut App) {}

/// Nothing, since there's no Dock.
pub(in crate::permissions) fn ask_for_badges(_: &mut App) -> Task<Badges> {
    Task::ready(Badges::Settled)
}
