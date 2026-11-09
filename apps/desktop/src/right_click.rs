//! What a right-click on a workspace's row or a chat's tab offers, built once so both offer it the same way.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Action, App, Context, Window};

use crate::workspace_pane::Mark;

/// Marking it read or unread, when `marking` says which and what that does, then renaming it. `marking_key` is the
/// action whose key marks what's showing, to show beside the item, for a menu whose key does the same.
pub(crate) fn menu(
    marking: Option<(Mark, impl Fn(&mut App) + 'static)>,
    marking_key: Option<Box<dyn Action>>,
    rename: impl Fn(&mut Window, &mut App) + 'static,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let marking = marking.map(|(marking, mark)| (marking, Rc::new(mark)));
    let rename = Rc::new(rename);
    move |menu, _, _| {
        let menu = match &marking {
            Some((marking, mark)) => {
                let mark = mark.clone();
                let icon = match marking {
                    Mark::Read => Lucide::MailOpen,
                    Mark::Unread => Lucide::Mail,
                };
                let item = PopupMenuItem::new(format!("Mark as {}", marking.word()))
                    .icon(icon)
                    .when_some(marking_key.as_ref(), |item, key| item.action(key.boxed_clone()))
                    .on_click(move |_, _, cx| mark(cx));
                menu.item(item)
            }
            None => menu,
        };
        let rename = rename.clone();
        menu.item(PopupMenuItem::new("Rename…").icon(Lucide::Pencil).on_click(move |_, window, cx| rename(window, cx)))
    }
}
