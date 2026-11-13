//! Where the window is, as Windows says, where GPUI's bounds for it are wrong. GPUI reads them from the window's
//! placement, in `WindowsWindowState::calculate_window_bounds` in `gpui-pre-windows`. For a window Windows has
//! arranged, by snapping it to an edge or a corner, or stretching it to its display's height, a placement gives where
//! the window goes back to once it's dragged away. For a minimized one, it says only that it's minimized, whether or
//! not the window goes back to filling the screen. And once Windows has destroyed the window, which can happen while
//! the app quits, before GPUI lets the window go, GPUI makes its bounds up from a placement of nothing.
//!
//! This moves GPUI's bounds by how far the window is from its placement, so it goes in the same GPUI upgrade that has
//! that function read the window's own rect for an ordinary window, as Chromium does. After that upgrade, it would
//! move them twice.
//!
//! What decides and converts is built for the tests everywhere, not only on Windows, since the app's unit tests don't
//! run on Windows.

#[cfg(any(target_os = "windows", test))]
use gpui_kit::{Bounds, Pixels, point, px, size};
use gpui_kit::{Window, WindowBounds};

#[cfg(test)]
mod tests;

/// The window's bounds, or `None` while it's minimized, or once Windows has no window to say anything of, when it still
/// goes back to where it was last seen. Windows gives no way to arrange a window, so one it had arranged opens as an
/// ordinary window, where it was.
#[cfg(target_os = "windows")]
pub(super) fn window_bounds(window: &Window) -> Option<WindowBounds> {
    let shown = crate::window_handle::hwnd(window).map_or(Shown::Gone, win32::shown);
    bounds(window.window_bounds(), shown, window.scale_factor())
}

/// Elsewhere, GPUI's bounds for the window are where it is.
#[cfg(not(target_os = "windows"))]
#[expect(clippy::unnecessary_wraps, reason = "it takes the place of Windows', which has none while minimized")]
pub(super) fn window_bounds(window: &Window) -> Option<WindowBounds> {
    Some(window.window_bounds())
}

/// A rect as Windows gives it, in its display's pixels: a window's, frame and all, or a display's.
#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(any(target_os = "windows", test))]
impl Rect {
    fn size(self) -> (i32, i32) {
        (self.right - self.left, self.bottom - self.top)
    }
}

/// How Windows says the window is shown.
#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug)]
enum Shown {
    Minimized,
    /// As an ordinary window, which Windows may have arranged: where its placement puts it, and where it is, both in
    /// a placement's terms.
    Normally {
        placed_at: Rect,
        now: Rect,
    },
    /// Maximized, which GPUI's bounds say rightly.
    Otherwise,
    /// Not at all, as Windows says of a window it has destroyed while the app quits, before GPUI lets it go.
    Gone,
}

/// The window's bounds, from what `gpui` gives and how Windows says the window is `shown`, at the window's `scale`.
/// A window counts as arranged when it isn't its placement's size, since a size is the same in a placement's terms as
/// the screen's. So an ordinary window keeps GPUI's bounds exactly, even if a placement's terms aren't what
/// [`in_placement_terms`] takes them to be. One arranged at its placement's size goes unnoticed, which is rare.
#[cfg(any(target_os = "windows", test))]
fn bounds(gpui: WindowBounds, shown: Shown, scale: f32) -> Option<WindowBounds> {
    match (gpui, shown) {
        (_, Shown::Minimized | Shown::Gone) => None,
        (WindowBounds::Windowed(goes_back_to), Shown::Normally { placed_at, now })
            if placed_at.size() != now.size() =>
        {
            Some(WindowBounds::Windowed(moved_to(goes_back_to, placed_at, now, scale)))
        }
        _ => Some(gpui),
    }
}

/// The bounds GPUI would give a window at `now`, from the `bounds` it gives one at `placed_at`. GPUI takes the
/// window's frame off its rect, which is the same at both, and divides by the window's `scale`, so this needs neither
/// how wide the frame is nor anything else of how GPUI goes from one to the other. What's saved stays in GPUI's terms,
/// since the window opens by giving GPUI those bounds back.
#[cfg(any(target_os = "windows", test))]
fn moved_to(bounds: Bounds<Pixels>, placed_at: Rect, now: Rect, scale: f32) -> Bounds<Pixels> {
    #[expect(clippy::cast_precision_loss, reason = "a display's pixels are far below where an f32 loses whole ones")]
    let by = |pixels: i32| px(pixels as f32 / scale);
    let ((width, height), (width_placed, height_placed)) = (now.size(), placed_at.size());
    Bounds::new(
        point(bounds.origin.x + by(now.left - placed_at.left), bounds.origin.y + by(now.top - placed_at.top)),
        size(bounds.size.width + by(width - width_placed), bounds.size.height + by(height - height_placed)),
    )
}

/// `rect`, a window's rect as the screen counts, in a placement's terms, which leave out a taskbar along the top or
/// left edge of the window's display, whose own rect is `display` and whose rect less its taskbar is `work_area`.
/// Chromium's `HWNDMessageHandler::GetWindowPlacement` goes between the two the same way.
#[cfg(any(target_os = "windows", test))]
fn in_placement_terms(rect: Rect, work_area: Rect, display: Rect) -> Rect {
    let (taskbar_left, taskbar_top) = (work_area.left - display.left, work_area.top - display.top);
    Rect {
        left: rect.left - taskbar_left,
        top: rect.top - taskbar_top,
        right: rect.right - taskbar_left,
        bottom: rect.bottom - taskbar_top,
    }
}

#[cfg(target_os = "windows")]
mod win32 {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, GetWindowRect, SW_SHOWMINIMIZED, SW_SHOWNORMAL, WINDOWPLACEMENT,
    };

    use super::{Rect, Shown, in_placement_terms};

    /// How the window `hwnd` is shown.
    pub(super) fn shown(hwnd: HWND) -> Shown {
        let length = u32::try_from(size_of::<WINDOWPLACEMENT>()).unwrap_or_default();
        let mut placement = WINDOWPLACEMENT { length, ..Default::default() };
        // SAFETY: it takes a window's handle, which Windows checks, so one it has destroyed only fails the call, and a
        // placement that outlives the call, with its length set as the call asks.
        if unsafe { GetWindowPlacement(hwnd, &raw mut placement) } == 0 {
            return Shown::Gone;
        }
        if placement.showCmd == SW_SHOWMINIMIZED as u32 {
            return Shown::Minimized;
        }
        if placement.showCmd != SW_SHOWNORMAL as u32 {
            return Shown::Otherwise;
        }
        let mut now = RECT::default();
        let display_length = u32::try_from(size_of::<MONITORINFO>()).unwrap_or_default();
        let mut display = MONITORINFO { cbSize: display_length, ..Default::default() };
        // SAFETY: each takes a window's handle or a display's, which Windows checks, and a struct that outlives the
        // call, with its size set where the call asks for it.
        let read = unsafe {
            GetWindowRect(hwnd, &raw mut now) != 0
                && GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &raw mut display) != 0
        };
        if !read {
            return Shown::Gone;
        }
        let now = in_placement_terms(rect(now), rect(display.rcWork), rect(display.rcMonitor));
        Shown::Normally { placed_at: rect(placement.rcNormalPosition), now }
    }

    fn rect(rect: RECT) -> Rect {
        Rect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }
    }
}
