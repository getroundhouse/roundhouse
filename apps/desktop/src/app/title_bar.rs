//! The window has no title bar, so the strip along its top, as tall as the workspace header, stands in for one:
//! dragging it moves the window, and double-clicking it does what double-clicking a title bar does, which on a Mac is
//! what its setting says, and on Windows maximizes or restores the window. A press there that a control took, such as
//! a button in the header, is the control's. On Windows it also holds the window's buttons.

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    App, Context, Div, FontFallbacks, Hsla, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement as _, Pixels, Role, Stateful, StatefulInteractiveElement as _, Styled as _, Window,
    WindowControlArea, div, font, px, rgb, white,
};

use super::AppView;
use crate::sides;
use crate::workspace_pane::HEADER_HEIGHT;

/// What Windows draws its window buttons with, in the font it draws them from: Segoe Fluent Icons on Windows 11, and
/// Segoe MDL2 Assets, which has the same, on Windows 10.
const MINIMIZE: &str = "\u{E921}";
const MAXIMIZE: &str = "\u{E922}";
const RESTORE: &str = "\u{E923}";
const CLOSE: &str = "\u{E8BB}";
const GLYPH_SIZE: Pixels = px(10.);
/// Windows' red behind the close button under the pointer.
const CLOSE_RED: u32 = 0x00C4_2B1C;

/// Who moves the window from the strip, which goes by the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleBar {
    /// The app, once a press on the strip moves, and it asks the system what a double click does. On a Mac and Linux.
    App,
    /// Windows, from its own hit test, which takes the strip for a title bar, and it maximizes or restores the window
    /// on a double click. GPUI's Windows backend can't start a move or answer a double click for the app.
    Windows,
}

impl TitleBar {
    const OF_THE_SYSTEM: Self = if cfg!(target_os = "windows") { Self::Windows } else { Self::App };
}

/// The window's, as the system has it, or as a test says in its place.
#[cfg(feature = "testing")]
fn title_bar(cx: &App) -> TitleBar {
    cx.try_global::<StandIn>().map_or(TitleBar::OF_THE_SYSTEM, |StandIn(title_bar)| *title_bar)
}

#[cfg(not(feature = "testing"))]
fn title_bar(_: &App) -> TitleBar {
    TitleBar::OF_THE_SYSTEM
}

/// What a test says of the title bar, in place of the system.
#[cfg(feature = "testing")]
struct StandIn(TitleBar);

#[cfg(feature = "testing")]
impl gpui_kit::Global for StandIn {}

/// Says the window's title bar is `title_bar` from the next frame on, whatever the system has, so a test can stand in
/// for Windows, where the tests mostly don't build.
#[cfg(feature = "testing")]
pub fn the_title_bar_is(title_bar: TitleBar, cx: &mut App) {
    cx.set_global(StandIn(title_bar));
}

/// `root`, the whole window, with the strip along its top. It goes in before what the window shows, so it's under
/// all of it, and what hides what's under it from the pointer hides the strip too: the kit's resize handle on a
/// sidebar's edge, a dialog's backdrop, and Home's search.
pub(super) fn stands_in(root: Stateful<Div>, cx: &mut Context<AppView>) -> Stateful<Div> {
    let title_bar = title_bar(cx);
    let strip = div().absolute().top_0().left_0().w_full().h(HEADER_HEIGHT).on_mouse_down(
        MouseButton::Left,
        cx.listener(move |t, event: &MouseDownEvent, window, cx| t.pressed_the_strip(title_bar, event, window, cx)),
    );
    match title_bar {
        // GPUI answers Windows' hit test as a title bar over what's marked so, and nowhere that hides it. A press
        // there is then one on the window's frame, which GPUI doesn't capture the mouse for, and while the pointer is
        // over the strip it says no button is held on each move. So what follows a held pointer in the strip, as a
        // text field does as it selects, hides the strip, as Home's search does.
        TitleBar::Windows => root.child(strip.window_control_area(WindowControlArea::Drag)),
        // Moves anywhere in the window, since the first can already be off the strip.
        TitleBar::App => root
            .child(strip)
            .on_mouse_move(cx.listener(|t, event: &MouseMoveEvent, window, _| {
                if std::mem::take(&mut t.moving_window) && event.pressed_button == Some(MouseButton::Left) {
                    window.start_window_move();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|t, _, _, _| t.moving_window = false)),
    }
}

impl AppView {
    /// A left press on the strip, which hears it after everything over it. One of the kit's buttons, or anything that
    /// takes focus, marks a press as taken, and a greyed-out button stops it, so it never gets here.
    fn pressed_the_strip(&mut self, title_bar: TitleBar, event: &MouseDownEvent, window: &mut Window, cx: &mut App) {
        if window.default_prevented() {
            // GPUI hands Windows a press on the strip once the app is done with it, unless it's stopped, and Windows
            // would take it for its title bar's and move the window.
            if title_bar == TitleBar::Windows {
                cx.stop_propagation();
            }
            return;
        }
        // Pressing a title bar leaves focus where it was.
        window.prevent_default();
        match title_bar {
            // Windows moves the window, once the press goes on to it.
            TitleBar::Windows => {}
            TitleBar::App => {
                self.moving_window = event.click_count != 2;
                if event.click_count == 2 {
                    window.titlebar_double_click();
                }
            }
        }
    }
}

/// Windows' minimize, maximize or restore, and close, at the strip's right end and as tall as it, or `None` where
/// `sides::end_room` leaves them no room. Windows works each itself, from what GPUI marks it as for Windows' hit test,
/// which also gives maximize the layouts to snap to. Each hides the strip and the window under it from the pointer,
/// so Windows takes a press on one as the button's rather than the strip's, and the app takes none of it.
pub(super) fn window_buttons(window: &Window, cx: &App) -> Option<impl IntoElement> {
    if sides::end_room(window) == px(0.) {
        return None;
    }
    let theme = cx.theme();
    // Dimmed while another window is active, as Windows dims its own.
    let glyph = if window.is_window_active() { theme.foreground } else { theme.muted_foreground };
    let (hover, pressed) = (theme.foreground.opacity(0.06), theme.foreground.opacity(0.04));
    let red: Hsla = rgb(CLOSE_RED).into();
    let (maximize, maximize_says) = if window.is_maximized() { (RESTORE, "Restore") } else { (MAXIMIZE, "Maximize") };
    let minimize = window_button("window-minimize", WindowControlArea::Min, MINIMIZE, "Minimize", glyph)
        .hover(move |style| style.bg(hover))
        .active(move |style| style.bg(pressed));
    let maximize = window_button("window-maximize", WindowControlArea::Max, maximize, maximize_says, glyph)
        .hover(move |style| style.bg(hover))
        .active(move |style| style.bg(pressed));
    let close = window_button("window-close", WindowControlArea::Close, CLOSE, "Close", glyph)
        .hover(move |style| style.bg(red).text_color(white()))
        .active(move |style| style.bg(red.opacity(0.9)).text_color(white()));
    Some(h_flex().absolute().top_0().right_0().h(HEADER_HEIGHT).child(minimize).child(maximize).child(close))
}

/// One of the window's buttons, drawn with `glyph` in `colour`, which says `says` to a screen reader.
fn window_button(
    id: &'static str,
    area: WindowControlArea,
    glyph: &'static str,
    says: &'static str,
    colour: Hsla,
) -> Stateful<Div> {
    let mut icons = font("Segoe Fluent Icons");
    icons.fallbacks = Some(FontFallbacks::from_fonts(vec!["Segoe MDL2 Assets".to_string()]));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(says)
        .w(sides::WINDOWS_BUTTON_WIDTH)
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .window_control_area(area)
        .font(icons)
        .text_size(GLYPH_SIZE)
        .text_color(colour)
        .child(glyph)
}
