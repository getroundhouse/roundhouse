//! Windows and Linux, which show no Permissions page, as `HAS_PERMISSIONS` says, so nothing here asks the system.

use std::collections::HashMap;

use gpui_kit::{App, Task};

use super::super::{Badges, Permission, Standing};

/// Nothing, since nothing reads it.
pub(in crate::permissions) fn read(_: &mut App) -> Task<HashMap<Permission, Standing>> {
    Task::ready(HashMap::new())
}

/// Nothing, since there's no page to ask from.
pub(in crate::permissions) fn ask(_: Permission, _: &mut App) -> Task<()> {
    Task::ready(())
}

/// Nothing, since there's no page to open it from.
pub(in crate::permissions) fn open_settings(_: Permission, _: &mut App) {}

/// Nothing, since only a Mac shows the Dock's badge.
pub(in crate::permissions) fn ask_for_badges(_: &mut App) -> Task<Badges> {
    Task::ready(Badges::Settled)
}
