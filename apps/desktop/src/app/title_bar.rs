//! The window has no title bar, so the strip along its top, as tall as the workspace header, stands in for one:
//! dragging it moves the window, and double-clicking it does what the system's setting says.

use gpui_kit::{
    Context, Div, InteractiveElement as _, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _, Stateful,
    Styled as _, WindowControlArea, div,
};

use super::AppView;
use crate::workspace_pane::HEADER_HEIGHT;

/// `root`, the whole window, moving the window from the strip. A press there that a control took, such as a button in
/// the header, is the control's. Every one of the kit's buttons, and anything that takes focus, marks its press as
/// taken before it gets here, since `root` is the outermost element. A press on a sidebar's edge never gets here at
/// all, because the kit's resize handle hides what's under it from the pointer.
pub(super) fn moves_window(root: Stateful<Div>, cx: &mut Context<AppView>) -> Stateful<Div> {
    if cfg!(target_os = "windows") { windows_moves_it(root, cx) } else { app_moves_it(root, cx) }
}

/// The app starts the move itself, once a press in the strip moves.
fn app_moves_it(root: Stateful<Div>, cx: &mut Context<AppView>) -> Stateful<Div> {
    root.on_mouse_down(
        MouseButton::Left,
        cx.listener(|t, event: &MouseDownEvent, window, _| {
            t.moving_window = event.position.y < HEADER_HEIGHT && !window.default_prevented();
            if !t.moving_window {
                return;
            }
            // Pressing a title bar leaves focus where it was.
            window.prevent_default();
            if event.click_count == 2 {
                t.moving_window = false;
                window.titlebar_double_click();
            }
        }),
    )
    .on_mouse_move(cx.listener(|t, event: &MouseMoveEvent, window, _| {
        if std::mem::take(&mut t.moving_window) && event.pressed_button == Some(MouseButton::Left) {
            window.start_window_move();
        }
    }))
    .on_mouse_up(MouseButton::Left, cx.listener(|t, _, _, _| t.moving_window = false))
}

/// Windows moves the window itself, and maximizes or restores it on a double click, since GPUI's Windows backend can't
/// start a move or answer a double click. It does both over whatever answers its hit test as a title bar, which GPUI
/// gives to what's marked `WindowControlArea::Drag`, here the whole strip. GPUI hands Windows' press there to the
/// app first, and back to Windows only if nothing stopped it, so a press a control took is stopped here.
///
/// Once a press over the strip is the app's, Windows tells it where the pointer goes as if nothing were held, so
/// what follows the pointer while it's held, as a text field does as it selects, hides the strip from the pointer
/// with `occlude`, as Home's search does.
fn windows_moves_it(root: Stateful<Div>, cx: &mut Context<AppView>) -> Stateful<Div> {
    // The window's first child, so it's under everything else, and whatever hides what's under it hides this too.
    let strip =
        div().absolute().top_0().left_0().w_full().h(HEADER_HEIGHT).window_control_area(WindowControlArea::Drag);
    root.child(strip).on_mouse_down(
        MouseButton::Left,
        cx.listener(|_, event: &MouseDownEvent, window, cx| {
            if event.position.y >= HEADER_HEIGHT {
                return;
            }
            if window.default_prevented() {
                cx.stop_propagation();
            } else {
                // Pressing a title bar leaves focus where it was.
                window.prevent_default();
            }
        }),
    )
}
