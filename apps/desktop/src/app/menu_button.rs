//! The button off the Mac that opens the app's menus, in the strip's top left corner, before the one that hides the
//! sidebar, where it stays, as that one does. F10 opens them too, as it opens a Windows or Linux app's menus. It keeps
//! the kit's popup menu itself, in the kit's `Popup`, rather than opening it with the kit's `dropdown_menu`, which
//! keeps whether it's open in the window's own state and opens it only from its button, so F10 couldn't.

use gpui_kit::base::Popup;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{IconName, Selectable as _};
use gpui_kit::{
    Anchor, ClickEvent, Context, DismissEvent, Entity, FocusHandle, Focusable as _, InteractiveElement as _,
    IntoElement, MouseButton, Subscription, Window, px,
};
use look::{Hand as _, Sizing as _};

use super::AppView;

/// The gap between the button and the menus under it.
const BELOW: gpui_kit::Pixels = px(4.);

/// The menus, while they're open.
pub(super) struct OpenMenus {
    menu: Entity<PopupMenu>,
    /// What had focus before they opened, which their items act on, and which has it again once they close.
    focus: Option<FocusHandle>,
    _closes: Subscription,
}

impl AppView {
    /// Opens the menus under the button, or closes them while they're open, as the button and F10 do.
    pub(super) fn toggle_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
        if let Some(open) = self.menus_open.take() {
            if let Some(focus) = open.focus {
                window.focus(&focus, cx);
            }
            return;
        }
        // What GPUI keeps of the menus where it has no menu bar for them, as they were last given.
        let Some(menus) = cx.get_menus() else { return };
        let focus = window.focused(cx);
        let menu = crate::menus::popup(menus, focus.clone(), window, cx);
        // Choosing an item or pressing Escape closes them, and puts focus back.
        let closes = cx.subscribe_in(&menu, window, |t, _, _: &DismissEvent, _, cx| {
            t.menus_open = None;
            cx.notify();
        });
        menu.focus_handle(cx).focus(window, cx);
        self.menus_open = Some(OpenMenus { menu, focus, _closes: closes });
    }
}

/// The button, with the menus under it while they're open. It opens them as it's pressed, as a menu bar does.
pub(super) fn button(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let open = app.menus_open.as_ref().map(|open| open.menu.clone());
    let button = Button::new("menus")
        .hand()
        .ghost()
        .sized_square(look::ControlSize::Small)
        .icon(IconName::Menu)
        .tooltip("Menu")
        .selected(open.is_some())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|t, _, window, cx| {
                // While the sidebar is hidden the button is over whatever is in the strip, such as an error, which
                // would take the press too, and the strip would move the window.
                window.prevent_default();
                cx.stop_propagation();
                t.toggle_menus(window, cx);
            }),
        )
        // Enter or Space on the button once Tab has reached it, which GPUI gives only what takes a click. A hand's
        // click opened the menus as it went down, and so did an assistive app's, which GPUI makes a press and a
        // release.
        .on_click(cx.listener(|t, event: &ClickEvent, window, cx| {
            if event.is_keyboard() {
                t.toggle_menus(window, cx);
            }
        }));
    let popup = Popup::new("menus-popup", button).anchor(Anchor::TopLeft).offset(BELOW);
    match open {
        Some(menu) => popup.content(menu),
        None => popup,
    }
}
