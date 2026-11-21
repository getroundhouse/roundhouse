//! What a workspace's menu offers in every app, as a right-click on the desktop's sidebar or a long press on the
//! phone's list opens it, and the rows a chat's menu shares with it. Each app does what's chosen.

use std::rc::Rc;

use client_core::marking::Mark;
use client_core::moving::doing;
use gpui_kit::assets::IconName;
use gpui_kit::{App, Window};
use protocol::model::Workspace;

use crate::mark_row;

/// What choosing a row of a workspace's or a chat's menu does, which the app that shows the menu does.
pub type Choose = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a workspace's menu does as each row is chosen, which the app that shows it does, and what that app is waiting
/// on the hub for, which the workspace's record doesn't say yet.
pub struct WorkspaceMenu {
    /// Marking its chats read or unread, when one has a reply, which `client_core::marking` says, and what that does.
    pub marking: Option<(Mark, Choose)>,
    pub rename: Choose,
    /// Asking the hub for a new title from what's been asked in it.
    pub retitle: Choose,
    /// Archiving it, or asking first where the app does.
    pub archive: Choose,
    /// A new title is on its way, so asking for another would do nothing.
    pub retitling: bool,
    /// The app is waiting on the hub for something that keeps it from archiving this, such as archiving it.
    pub archiving: bool,
}

impl WorkspaceMenu {
    /// The rows for `workspace` as its record has it now: what a chat's menu offers, a new title, then archiving it,
    /// apart and last, as a row that removes something is. A new title needs one to replace, which the first prompt
    /// gives, and neither a new title nor archiving can start while the workspace is on its way out or back.
    pub fn of(self, workspace: &Workspace) -> impl Fn(look::Menu, &mut Window, &mut App) -> look::Menu + 'static {
        let WorkspaceMenu { marking, rename, retitle, archive, retitling, archiving } = self;
        let going = doing(workspace.status).is_some();
        let cant_retitle = going || retitling || workspace.title.is_none();
        let cant_archive = going || archiving;
        let shared = marking_and_renaming(marking, rename);
        move |menu, window, cx| {
            let (retitle, archive) = (retitle.clone(), archive.clone());
            let regenerate = look::menu_row("Regenerate title").icon(IconName::PencilSparkles).off(cant_retitle);
            let menu = shared(menu, window, cx).row(regenerate.on_choose(move |window, cx| retitle(window, cx)));
            let row = look::menu_row("Archive").icon(IconName::Archive).removes().off(cant_archive);
            menu.separator().row(row.on_choose(move |window, cx| archive(window, cx)))
        }
    }
}

/// Marking it read or unread, when `marking` says which and what that does, then renaming it: what a chat's menu
/// offers, and a workspace's starts with.
pub fn marking_and_renaming(
    marking: Option<(Mark, Choose)>,
    rename: Choose,
) -> impl Fn(look::Menu, &mut Window, &mut App) -> look::Menu + 'static {
    move |menu, _, _| {
        let menu = match &marking {
            Some((mark, does)) => {
                let does = does.clone();
                menu.row(mark_row(*mark).on_choose(move |window, cx| does(window, cx)))
            }
            None => menu,
        };
        let rename = rename.clone();
        menu.row(look::menu_row("Rename…").icon(IconName::Pencil).on_choose(move |window, cx| rename(window, cx)))
    }
}
