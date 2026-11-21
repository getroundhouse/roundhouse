//! Whether a hover card shows, and what keeps it open: the pointer, or a tap under touch, the keyboard's focus, and
//! what's opened from it, which holds it open.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{
    App, Bounds, Context, DismissEvent, Entity, EntityId, EventEmitter, FocusHandle, Pixels, Point, Subscription, Task,
    WeakEntity, Window,
};

use super::card::OpenChange;

/// Holds a card open, whatever the pointer does, while something opened from it is, such as a menu opened from a
/// button in it. It's for the time the card was open when it was handed out, so one kept after the card has closed
/// does nothing, even once the card opens again.
#[derive(Clone)]
pub struct HoldOpen {
    pub(super) shown: WeakEntity<Shown>,
    pub(super) opening: usize,
}

impl HoldOpen {
    /// Holds the card open while `view` is open: until it's dismissed, or goes without that, as a menu does when the
    /// button it opened from is no longer drawn. Once nothing holds the card, it goes as it would have, unless the
    /// pointer is on the card or what it's for.
    pub fn while_open<V: EventEmitter<DismissEvent>>(&self, view: &Entity<V>, window: &Window, cx: &mut App) {
        // Gone once the card's trigger is no longer drawn, when there's no card left to hold.
        let Some(shown) = self.shown.upgrade() else { return };
        let (opening, id) = (self.opening, view.entity_id());
        shown.update(cx, |shown, cx| {
            if !shown.open || shown.opening != opening {
                return;
            }
            shown.holders.push(id);
            cx.subscribe_in(view, window, move |shown, _, _: &DismissEvent, window, cx| shown.let_go(id, window, cx))
                .detach();
            cx.observe_release_in(view, window, move |shown, _, window, cx| shown.let_go(id, window, cx)).detach();
        });
    }

    /// Holds the card open while `menu` is, as [`HoldOpen::while_open`] does, such as a menu opened from a button on
    /// the card, which is drawn outside it.
    pub fn while_menu_open(&self, menu: &crate::OpenMenu, window: &Window, cx: &mut App) {
        if let Some(menu) = menu.kit() {
            self.while_open(&menu, window, cx);
        }
    }
}

/// Whether a card shows, and what decides when it next does or goes, kept from one drawing to the next.
#[derive(Default)]
pub(super) struct Shown {
    pub(super) open: bool,
    pub(super) open_delay: Duration,
    pub(super) close_delay: Duration,
    pub(super) on_open_change: Option<OpenChange>,
    /// When it next opens or closes. A newer one replaces it, and dropping it cancels it.
    pub(super) timer: Option<Task<()>>,
    /// Where the pointer and the keyboard's focus are, which keep it open.
    on: On,
    /// What takes the keyboard's focus for a card that shows on it, made the first time it's drawn so, with what says
    /// when the focus comes and goes.
    pub(super) focus: Option<(FocusHandle, [Subscription; 2])>,
    /// Where what it's for was last drawn, for a card that flips to fit.
    pub(super) trigger_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// What holds it open. It only closes once nothing does.
    pub(super) holders: Vec<EntityId>,
    /// Counts each time it opens, so a hold handed out an earlier time does nothing.
    pub(super) opening: usize,
}

/// Which of what keeps a card open, but a hold, it's on: the pointer on what it's for, the pointer on the card, and the
/// keyboard's focus on what it's for, put there from the keyboard.
#[derive(Clone, Copy, Default)]
struct On {
    trigger: bool,
    card: bool,
    focus: bool,
}

impl Shown {
    pub(super) fn after(&mut self, delay: Duration, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |shown, window, cx| {
                if open || !shown.stays() {
                    shown.set_open(open, window, cx);
                }
            });
        }));
    }

    /// Whether something keeps it open: the pointer, on the card or what it's for, the keyboard's focus, or what holds
    /// it.
    pub(super) fn stays(&self) -> bool {
        self.on.trigger || self.on.card || self.on.focus || !self.holders.is_empty()
    }

    /// Has it go after its delay, unless something keeps it open.
    pub(super) fn close_unless_kept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.stays() {
            self.after(self.close_delay, false, window, cx);
        }
    }

    pub(super) fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.open == open {
            return;
        }
        self.open = open;
        if open {
            self.opening += 1;
        }
        cx.notify();
        // From here rather than from the element, since a timer outlives the element that set it.
        if let Some(changed) = self.on_open_change.clone() {
            changed(&open, window, cx);
        }
    }

    pub(super) fn on_trigger(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.on.trigger = on;
        if on {
            self.after(self.open_delay, true, window, cx);
        } else {
            self.close_unless_kept(window, cx);
        }
    }

    /// A tap on what it's for, under touch, which shows it, or puts it away if it shows.
    pub(super) fn tapped(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.timer = None;
        let open = !self.open;
        self.set_open(open, window, cx);
    }

    /// A press anywhere but the card, under touch, which puts it away, unless it's on what it's for, whose tap does.
    pub(super) fn pressed_off_the_card(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let on_trigger = self.trigger_at.get().is_some_and(|trigger| trigger.contains(&at));
        if !on_trigger && self.holders.is_empty() {
            self.timer = None;
            self.set_open(false, window, cx);
        }
    }

    pub(super) fn on_card(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.on.card = on;
        if on {
            self.timer = None;
        } else {
            self.close_unless_kept(window, cx);
        }
    }

    /// What takes the keyboard's focus for it, made the first time it's asked for.
    pub(super) fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) -> FocusHandle {
        if let Some((focus, _)) = &self.focus {
            return focus.clone();
        }
        let focus = cx.focus_handle().tab_stop(true);
        // Only from the keyboard, since a click is the pointer's, which shows it once it has rested.
        let came = cx.on_focus_in(&focus, window, |shown, window, cx| {
            shown.focused(window.last_input_was_keyboard(), window, cx);
        });
        let went = cx.on_focus_out(&focus, window, |shown, _, window, cx| shown.focused(false, window, cx));
        self.focus = Some((focus.clone(), [came, went]));
        focus
    }

    pub(super) fn focused(&mut self, focused: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.on.focus = focused;
        if focused {
            self.timer = None;
            self.set_open(true, window, cx);
        } else {
            self.close_unless_kept(window, cx);
        }
    }

    /// `holder` no longer holds it open, being dismissed or gone, if it still did.
    pub(super) fn let_go(&mut self, holder: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let held = self.holders.len();
        self.holders.retain(|id| *id != holder);
        if self.holders.len() < held {
            self.close_unless_kept(window, cx);
        }
    }
}
