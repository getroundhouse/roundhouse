//! The window's handle on Windows, for what the app asks of Windows itself, where GPUI doesn't.

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::HWND;

/// `window`'s handle, or `None` while it has none.
pub(crate) fn hwnd(window: &Window) -> Option<HWND> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return None };
    Some(handle.hwnd.get() as HWND)
}
