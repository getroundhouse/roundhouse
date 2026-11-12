//! Holding a list's order still while someone is using it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{
    Context, DismissEvent, Div, Entity, InteractiveElement as _, MouseMoveEvent, Stateful,
    StatefulInteractiveElement as _, Task, WeakEntity,
};
use protocol::model::{Workspace, WorkspaceId};

use crate::app::AppView;

/// How long the order holds still after the pointer last moved over the list: long enough to finish a click it was on
/// its way to, and short enough that a pointer left resting there while someone types doesn't keep the order from
/// changing.
pub(crate) const HOLDS_FOR: Duration = Duration::from_millis(1500);

/// Where the app keeps a list's [`Hold`].
type Holder<V> = fn(&mut AppView) -> &mut Hold<V>;

/// What holds a list's order still while someone is using it. `V` is what the list is showing, such as the order
/// chosen, and the order holds only while that stays the same, so choosing another takes at once.
pub(crate) struct Hold<V> {
    /// The pointer is over the list.
    pointing: bool,
    /// When the pointer last moved over the list, by the app's clock.
    moved_at: Option<Instant>,
    /// Draws the list again once the pointer has rested for [`HOLDS_FOR`], so the order the hold kept back shows.
    releasing: Option<Task<()>>,
    /// The menu opened from a row, while it's open.
    row_menu: Option<WeakEntity<PopupMenu>>,
    /// The order the workspaces were last shown in, and what the list was showing then.
    shown: Option<(V, Vec<WorkspaceId>)>,
}

impl<V> Default for Hold<V> {
    fn default() -> Self {
        Self { pointing: false, moved_at: None, releasing: None, row_menu: None, shown: None }
    }
}

impl<V: PartialEq> Hold<V> {
    /// The order to hold the workspaces in, as they were last shown, while someone is using the list, at `now`: for
    /// [`HOLDS_FOR`] after the pointer last moved over it, and while a row's menu is open. A row's menu is drawn by
    /// the kit only while its row is under where it was opened, so it would go if its row moved. Only while the list
    /// is `showing` what it showed then.
    pub(crate) fn held(&self, showing: &V, now: Instant) -> Option<&[WorkspaceId]> {
        let moving = self.pointing && self.moved_at.is_some_and(|at| now < at + HOLDS_FOR);
        let menu = self.row_menu.as_ref().is_some_and(|menu| menu.upgrade().is_some());
        let shown = self.shown.as_ref().filter(|(shown, _)| (moving || menu) && shown == showing);
        shown.map(|(_, order)| order.as_slice())
    }

    pub(crate) fn showed(&mut self, showing: V, order: Vec<WorkspaceId>) {
        self.shown = Some((showing, order));
    }

    /// What the list was showing when it was last shown, once it has been.
    pub(crate) fn last_showing(&self) -> Option<&V> {
        self.shown.as_ref().map(|(showing, _)| showing)
    }
}

impl<V> Hold<V> {
    /// The list isn't drawn this frame, such as while Settings shows, so it hears nothing of the pointer, and the
    /// pointer can't be taken to be on it once it's drawn again.
    pub(crate) fn not_drawn(&mut self) {
        self.pointing = false;
    }

    /// How much longer the order holds for the pointer, at `now`, or `None` once it doesn't.
    fn hold_left(&self, now: Instant) -> Option<Duration> {
        let left = self.moved_at?.checked_add(HOLDS_FOR)?.saturating_duration_since(now);
        (!left.is_zero()).then_some(left)
    }
}

/// `room`, a box that fills the same room as a list, hearing the pointer for the list's hold, since the list can't
/// hear it itself.
pub(crate) fn hears_the_pointer<V: 'static>(
    room: Stateful<Div>,
    hold: Holder<V>,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    room.on_hover(cx.listener(move |t, pointing: &bool, _, cx| {
        hold(t).pointing = *pointing;
        cx.notify();
    }))
    .on_mouse_move(cx.listener(move |t, _: &MouseMoveEvent, _, cx| pointer_moved(t, hold, cx)))
}

/// The pointer moved over the list, which holds the order still for a moment, and draws it again once it's rested.
fn pointer_moved<V: 'static>(app: &mut AppView, hold: Holder<V>, cx: &mut Context<AppView>) {
    hold(app).moved_at = Some(cx.background_executor().now());
    if hold(app).releasing.is_some() {
        return;
    }
    let releasing = cx.spawn(async move |this, cx| {
        // However often it moves meanwhile, until it has rested for long enough.
        while let Ok(Some(left)) = this.update(cx, |t, cx| hold(t).hold_left(cx.background_executor().now())) {
            cx.background_executor().timer(left).await;
        }
        let _ = this.update(cx, |t, cx| {
            hold(t).releasing = None;
            cx.notify();
        });
    });
    hold(app).releasing = Some(releasing);
}

/// Holds the order still while `menu`, which a right-click on one of the list's rows opened, is open: until it's
/// dismissed, or goes without that, as it does when its row is no longer drawn.
pub(crate) fn hold_while_open<V: 'static>(
    app: &mut AppView,
    menu: &Entity<PopupMenu>,
    hold: Holder<V>,
    cx: &mut Context<AppView>,
) {
    hold(app).row_menu = Some(menu.downgrade());
    let id = menu.entity_id();
    let ends = move |app: &mut AppView, cx: &mut Context<AppView>| {
        let open = &mut hold(app).row_menu;
        if open.as_ref().is_some_and(|open| open.entity_id() == id) {
            *open = None;
            cx.notify();
        }
    };
    cx.subscribe(menu, move |app, _, _: &DismissEvent, cx| ends(app, cx)).detach();
    cx.observe_release(menu, move |app, _, cx| ends(app, cx)).detach();
}

/// Puts `listed`, places in `workspaces`, in the order `held` gives their IDs, and any it doesn't name after them, in
/// the order they're in, so a workspace that comes while the order holds doesn't push the others along.
pub(crate) fn keep_held(listed: &mut [usize], workspaces: &[Workspace], held: &[WorkspaceId]) {
    let place: HashMap<&WorkspaceId, usize> = held.iter().enumerate().map(|(at, id)| (id, at)).collect();
    listed.sort_by_key(|&w| place.get(&workspaces[w].id).copied().unwrap_or(usize::MAX));
}

#[cfg(test)]
mod tests;
