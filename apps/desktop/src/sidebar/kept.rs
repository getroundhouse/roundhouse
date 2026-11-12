//! What the sidebar keeps from one frame to the next, besides where it's scrolled to: where each line is on its way,
//! what holds the order still while someone is using the list, and which workspaces there were before one was made.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{Context, DismissEvent, Entity, Task, WeakEntity};
use protocol::model::{Workspace, WorkspaceId};

use super::places::Places;
use crate::app::AppView;
use crate::workspace_order::Sort;

/// How long the order holds still after the pointer last moved over the list: long enough to finish a click it was on
/// its way to, and short enough that a pointer left resting there while someone types doesn't keep the order from
/// changing.
pub(crate) const HOLDS_FOR: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub(crate) struct Kept {
    /// Where each line is going, and has got to, so one that moves slides there.
    pub(super) places: Places,
    /// The menu of orders is open, so the button it's from shows wherever the pointer is.
    pub(super) sort_menu_open: bool,
    /// The pointer is over the list.
    pub(super) pointing: bool,
    /// When the pointer last moved over the list, by the app's clock.
    moved_at: Option<Instant>,
    /// Draws the list again once the pointer has rested for [`HOLDS_FOR`], so the order the hold kept back shows.
    releasing: Option<Task<()>>,
    /// The menu opened from a row, while it's open.
    row_menu: Option<WeakEntity<PopupMenu>>,
    /// The order the workspaces were last shown in, and the order chosen then.
    shown: Option<(Sort, Vec<WorkspaceId>)>,
    /// Every workspace there was as the one being made was asked for, so the one that's new is the one not here.
    before_making: Option<HashSet<WorkspaceId>>,
}

impl Kept {
    /// The order to hold the workspaces in, as they were last shown, while someone is using the list: for
    /// [`HOLDS_FOR`] after the pointer last moved over it, and while a row's menu is open. So a row doesn't move out
    /// from under a click, and a row's menu, which the kit only draws while its row is under where it was opened,
    /// doesn't go. Only while the order chosen is the one they were shown in, so choosing another, from the heading,
    /// which is in the list, takes at once.
    pub(super) fn held(&self, sort: Sort, now: Instant) -> Option<&[WorkspaceId]> {
        let moving = self.pointing && self.moved_at.is_some_and(|at| now < at + HOLDS_FOR);
        let menu = self.row_menu.as_ref().is_some_and(|menu| menu.upgrade().is_some());
        let shown = self.shown.as_ref().filter(|(shown, _)| (moving || menu) && *shown == sort);
        shown.map(|(_, order)| order.as_slice())
    }

    pub(super) fn showed(&mut self, sort: Sort, order: Vec<WorkspaceId>) {
        self.shown = Some((sort, order));
    }

    /// The list isn't drawn this frame, such as while Settings shows, so it hears nothing of the pointer or of the
    /// menu of orders, and neither can be taken to be on it once it's drawn again.
    pub(crate) fn not_drawn(&mut self) {
        self.pointing = false;
        self.sort_menu_open = false;
    }

    /// A workspace is about to be asked for, among `workspaces`.
    pub(crate) fn making_among(&mut self, workspaces: &[Workspace]) {
        self.before_making = Some(workspaces.iter().map(|w| w.id.clone()).collect());
    }

    /// Whether the workspace being made is among `listed`, places in `workspaces`, so it takes the note's place.
    pub(super) fn made_is_listed(&self, workspaces: &[Workspace], listed: &[usize]) -> bool {
        let Some(before) = &self.before_making else { return false };
        listed.iter().any(|&w| !before.contains(&workspaces[w].id))
    }

    /// How much longer the order holds for the pointer, at `now`, or `None` once it doesn't.
    fn hold_left(&self, now: Instant) -> Option<Duration> {
        let left = self.moved_at?.checked_add(HOLDS_FOR)?.saturating_duration_since(now);
        (!left.is_zero()).then_some(left)
    }
}

/// The pointer moved over the list, which holds the order still for a moment, and draws it again once it's rested.
pub(super) fn pointer_moved(app: &mut AppView, cx: &mut Context<AppView>) {
    app.sidebar.moved_at = Some(cx.background_executor().now());
    if app.sidebar.releasing.is_some() {
        return;
    }
    app.sidebar.releasing = Some(cx.spawn(async move |this, cx| {
        // However often it moves meanwhile, until it has rested for long enough.
        while let Ok(Some(left)) = this.read_with(cx, |t, cx| t.sidebar.hold_left(cx.background_executor().now())) {
            cx.background_executor().timer(left).await;
        }
        let _ = this.update(cx, |t, cx| {
            t.sidebar.releasing = None;
            cx.notify();
        });
    }));
}

/// Holds the order still while `menu`, which a right-click on a row opened, is open: until it's dismissed, or goes
/// without that, as it does when its row is no longer drawn.
pub(super) fn hold_while_open(app: &mut AppView, menu: &Entity<PopupMenu>, cx: &mut Context<AppView>) {
    app.sidebar.row_menu = Some(menu.downgrade());
    let id = menu.entity_id();
    let ends = move |app: &mut AppView, cx: &mut Context<AppView>| {
        if app.sidebar.row_menu.as_ref().is_some_and(|held| held.entity_id() == id) {
            app.sidebar.row_menu = None;
            cx.notify();
        }
    };
    cx.subscribe(menu, move |app, _, _: &DismissEvent, cx| ends(app, cx)).detach();
    cx.observe_release(menu, move |app, _, cx| ends(app, cx)).detach();
}

/// Puts `listed`, places in `workspaces`, in the order `held` gives their IDs, and any it doesn't name after them, in
/// the order they're in, so a workspace that comes while the order holds doesn't push the others along.
pub(super) fn hold(listed: &mut [usize], workspaces: &[Workspace], held: &[WorkspaceId]) {
    let place: HashMap<&WorkspaceId, usize> = held.iter().enumerate().map(|(at, id)| (id, at)).collect();
    listed.sort_by_key(|&w| place.get(&workspaces[w].id).copied().unwrap_or(usize::MAX));
}

#[cfg(test)]
mod tests;
