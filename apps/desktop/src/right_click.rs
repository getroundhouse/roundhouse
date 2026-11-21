//! What a right-click on a workspace's row or a chat's tab offers, built once so both offer it the same way.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::{App, Window};

use crate::workspace_pane::Mark;

/// Marking it read or unread, when `marking` says which and what that does, then renaming it.
pub(crate) fn menu(
    marking: Option<(Mark, impl Fn(&mut App) + 'static)>,
    rename: impl Fn(&mut Window, &mut App) + 'static,
) -> impl Fn(look::Menu, &mut Window, &mut App) -> look::Menu + 'static {
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
                let row =
                    look::menu_row(format!("Mark as {}", marking.word())).icon(icon).on_choose(move |_, cx| mark(cx));
                menu.row(row)
            }
            None => menu,
        };
        let rename = rename.clone();
        menu.row(look::menu_row("Rename…").icon(Lucide::Pencil).on_choose(move |window, cx| rename(window, cx)))
    }
}
