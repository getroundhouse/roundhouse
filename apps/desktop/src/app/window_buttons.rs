//! Windows' minimize, maximize or restore, and close, which the app draws at the right end of the strip along the
//! window's top. The app hides the system's title bar, and GPUI's Windows backend hides its buttons with it.

use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};
use gpui_kit::{
    AccessibleAction, App, Font, Global, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, WindowControlArea, deferred, div, font,
    px, rgb, white,
};

use crate::sides::{self, WindowButtons};
use crate::workspace_pane::HEADER_HEIGHT;

/// Windows' icon fonts, newest first: Segoe Fluent Icons on Windows 11, and Segoe MDL2 Assets, which has the same
/// glyphs at the same code points, on Windows 10.
const ICON_FONTS: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];
const GLYPH_SIZE: Pixels = px(10.);
/// Windows' red behind the close button under the pointer.
const CLOSE_RED: u32 = 0x00C4_2B1C;
/// Over a dialog, which the kit draws from 10 up over a backdrop that's a title bar to Windows, so the buttons still
/// work while one is open, and under the menus and tooltips it draws at 100 and 200.
const PRIORITY: usize = 50;

/// One of the window's buttons.
#[derive(Clone, Copy)]
enum Button {
    Minimize,
    Maximize,
    /// Maximize, while the window is maximized.
    Restore,
    Close,
}

impl Button {
    fn id(self) -> &'static str {
        match self {
            Button::Minimize => "window-minimize",
            Button::Maximize | Button::Restore => "window-maximize",
            Button::Close => "window-close",
        }
    }

    fn area(self) -> WindowControlArea {
        match self {
            Button::Minimize => WindowControlArea::Min,
            Button::Maximize | Button::Restore => WindowControlArea::Max,
            Button::Close => WindowControlArea::Close,
        }
    }

    /// What Windows draws it with, in its icon fonts.
    fn glyph(self) -> &'static str {
        match self {
            Button::Minimize => "\u{E921}",
            Button::Maximize => "\u{E922}",
            Button::Restore => "\u{E923}",
            Button::Close => "\u{E8BB}",
        }
    }

    /// The kit's icon for it, where neither of Windows' icon fonts is installed.
    fn icon(self) -> IconName {
        match self {
            Button::Minimize => IconName::WindowMinimize,
            Button::Maximize => IconName::WindowMaximize,
            Button::Restore => IconName::WindowRestore,
            Button::Close => IconName::WindowClose,
        }
    }

    /// What it's called to a screen reader.
    fn says(self) -> &'static str {
        match self {
            Button::Minimize => "Minimize",
            Button::Maximize => "Maximize",
            Button::Restore => "Restore",
            Button::Close => "Close",
        }
    }
}

/// The three, at the strip's right end and as tall as it, where `sides` says they are, which is on Windows out of full
/// screen. GPUI answers Windows' hit test over each with the button it's marked as, and its Windows backend minimizes,
/// maximizes, restores or closes the window as a press there is let go. Each hides the strip and the window under it
/// from the pointer, so the press is the button's rather than the strip's, and nothing in the app hears it. They're
/// drawn after everything else, a dialog included.
pub(super) fn of_windows(window: &Window, cx: &mut App) -> Option<impl IntoElement> {
    if sides::window_buttons(cx) != WindowButtons::AtTheEnd {
        return None;
    }
    let glyphs = Glyphs::of(cx);
    let theme = cx.theme();
    // Dimmed while another window is active, as Windows dims its own.
    let colour = if window.is_window_active() { theme.foreground } else { theme.muted_foreground };
    let faint = Shade { hover: theme.foreground.opacity(0.06), pressed: theme.foreground.opacity(0.04), text: colour };
    let red: Hsla = rgb(CLOSE_RED).into();
    let red = Shade { hover: red, pressed: red.opacity(0.9), text: white() };
    let drawn = |button: Button, shade: Shade| drawn(button, glyphs.as_ref(), colour, shade);
    let maximize = if window.is_maximized() { Button::Restore } else { Button::Maximize };
    // A third of the room `sides` leaves them each.
    let buttons = h_flex()
        .w(sides::WINDOWS_BUTTONS_WIDTH)
        .h(HEADER_HEIGHT)
        .child(drawn(Button::Minimize, faint))
        .child(drawn(maximize, faint))
        .child(drawn(Button::Close, red));
    Some(div().absolute().top_0().right_0().child(deferred(buttons).with_priority(PRIORITY)))
}

/// What a button's background is under the pointer and while it's pressed, and its glyph's colour then.
#[derive(Clone, Copy)]
struct Shade {
    hover: Hsla,
    pressed: Hsla,
    text: Hsla,
}

/// `button`, drawn with `glyphs`, Windows' icon font, in `colour`, or with the kit's icon without it, and in `shade`
/// under the pointer. A screen reader presses it by its accessible action, since it has no click of its own.
fn drawn(button: Button, glyphs: Option<&Font>, colour: Hsla, shade: Shade) -> impl IntoElement {
    let drawn = div()
        .id(button.id())
        .role(Role::Button)
        .aria_label(button.says())
        .on_a11y_action(AccessibleAction::Click, move |_, window, _| system::press(button, window))
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .window_control_area(button.area())
        .text_color(colour)
        .hover(move |style| style.bg(shade.hover).text_color(shade.text))
        .active(move |style| style.bg(shade.pressed).text_color(shade.text));
    let drawn = match glyphs {
        Some(glyphs) => drawn.font(glyphs.clone()).text_size(GLYPH_SIZE).child(button.glyph()),
        None => drawn.child(Icon::new(button.icon()).small()),
    };
    drawn.test_support()
}

/// Windows' icon font that's installed, found the first time the buttons are drawn, or `None` with neither. GPUI
/// doesn't try a font's fallbacks when the font itself is missing, so the one that's there is chosen here.
struct Glyphs(Option<Font>);

impl Global for Glyphs {}

impl Glyphs {
    fn of(cx: &mut App) -> Option<Font> {
        if let Some(Glyphs(found)) = cx.try_global::<Glyphs>() {
            return found.clone();
        }
        let installed = cx.text_system().all_font_names();
        let found = ICON_FONTS.into_iter().find(|family| installed.iter().any(|name| name == family)).map(font);
        cx.set_global(Glyphs(found.clone()));
        found
    }
}

#[cfg(target_os = "windows")]
mod system {
    use gpui_kit::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PostMessageW, SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_RESTORE, WM_SYSCOMMAND,
    };

    use super::Button;

    /// As Windows' own buttons do, with the command its menu for the window gives. Close asks the window to close, as
    /// the button does, so quitting asks first about what it would lose.
    pub(super) fn press(button: Button, window: &Window) {
        let command = match button {
            Button::Minimize => SC_MINIMIZE,
            Button::Maximize => SC_MAXIMIZE,
            Button::Restore => SC_RESTORE,
            Button::Close => SC_CLOSE,
        };
        let Ok(handle) = HasWindowHandle::window_handle(window) else { return };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else { return };
        let hwnd = handle.hwnd.get() as HWND;
        // SAFETY: it takes this window's handle, which is open while the window is drawn, and a message that carries
        // no pointer.
        unsafe { PostMessageW(hwnd, WM_SYSCOMMAND, command as usize, 0) };
    }
}

#[cfg(not(target_os = "windows"))]
mod system {
    /// Only Windows draws the buttons, and a test that stands in for it elsewhere can't ask for an accessible action.
    pub(super) fn press(_: super::Button, _: &gpui_kit::Window) {}
}
