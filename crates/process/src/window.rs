use std::process::Command;

/// Keeps `command` from opening a console window on Windows, as a console program started by one without a console,
/// such as the app, otherwise does for as long as it runs. It still gets a console, with no window, so what it writes
/// can still be read. On Unix it does nothing, since a program there has no window unless it opens one.
///
/// It sets the command's creation flags, which takes the place of any set before, such as [`own_group`]'s, so it
/// can't be used with them.
///
/// [`own_group`]: crate::own_group
pub fn without_window(command: &mut Command) {
    crate::sys::without_window(command);
}
