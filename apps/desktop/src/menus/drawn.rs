//! The menus as the app draws them off the Mac, from what GPUI keeps of them there: one of the kit's popup menus, with
//! each menu a submenu of it. `with_items` is the kit's `PopupMenu::with_menu_items` again, which the kit keeps to
//! itself, with each item greyed out where nothing would take its action, as macOS greys it out.

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{Action, App, Context, Entity, FocusHandle, OwnedMenu, OwnedMenuItem, Window};

/// `menus` as one popup menu, each a submenu of it. Their items act on what has `focus`, as the menu bar's act on
/// what has focus, and an item that nothing there would take is greyed out, as macOS greys it out.
pub(crate) fn popup(
    menus: Vec<OwnedMenu>,
    focus: Option<FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PopupMenu> {
    PopupMenu::build(window, cx, move |mut popup, window, cx| {
        if let Some(focus) = &focus {
            popup = popup.action_context(focus.clone());
        }
        for menu in menus {
            let focus = focus.clone();
            popup = popup.submenu(menu.name, window, cx, move |submenu, window, cx| {
                with_items(submenu, &menu.items, focus.as_ref(), window, cx)
            });
        }
        popup
    })
}

/// `menu` with `items` in it, each acting on what has `focus`.
fn with_items(
    mut menu: PopupMenu,
    items: &[OwnedMenuItem],
    focus: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    for item in items {
        menu = match item {
            OwnedMenuItem::Action { name, action, checked, disabled, .. } => {
                let off = !can_act(action.as_ref(), focus, window, cx);
                let item = PopupMenuItem::new(name.clone()).action(action.boxed_clone());
                menu.item(item.disabled(off || *disabled).checked(*checked))
            }
            OwnedMenuItem::Separator => menu.separator(),
            OwnedMenuItem::Submenu(submenu) => {
                let (items, focus) = (submenu.items.clone(), focus.cloned());
                menu.submenu(submenu.name.clone(), window, cx, move |menu, window, cx| {
                    with_items(menu, &items, focus.as_ref(), window, cx)
                })
            }
            // macOS's own, such as Services, which only a Mac has.
            OwnedMenuItem::SystemMenu(_) => menu,
        };
    }
    menu
}

/// Whether anything takes `action` where `focus` is, as macOS asks before it shows an item: the views it goes through
/// on its way up from there, or the app, wherever focus is.
fn can_act(action: &dyn Action, focus: Option<&FocusHandle>, window: &Window, cx: &mut App) -> bool {
    let in_the_window = match focus {
        Some(focus) => window.is_action_available_in(action, focus),
        None => window.is_action_available(action, cx),
    };
    // GPUI asks the window in front too, which it can't reach while it's being updated, as it is while it opens the
    // menus, so this is only what the app takes.
    in_the_window || cx.is_action_available(action)
}
