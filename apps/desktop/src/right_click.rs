//! What a right-click on a workspace's row or a chat's tab offers, built once so both offer it the same way.

use std::rc::Rc;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, Context, Window};

use crate::workspace_pane::Mark;

/// Marking it read or unread, when `marking` says which and what that does, then renaming it.
pub(crate) fn menu(
    marking: Option<(Mark, impl Fn(&mut App) + 'static)>,
    rename: impl Fn(&mut Window, &mut App) + 'static,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let marking = marking.map(|(marking, mark)| (marking, Rc::new(mark)));
    let rename = Rc::new(rename);
    move |menu, _, _| {
        let menu = match &marking {
            Some((marking, mark)) => {
                let mark = mark.clone();
                menu.item(PopupMenuItem::new(format!("Mark as {}", marking.word())).on_click(move |_, _, cx| mark(cx)))
            }
            None => menu,
        };
        let rename = rename.clone();
        menu.item(PopupMenuItem::new("Rename…").on_click(move |_, window, cx| rename(window, cx)))
    }
}
