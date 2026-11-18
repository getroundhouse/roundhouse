//! The app's hover card: it shows once the pointer has rested on what it's for, and goes once the pointer has left
//! both that and the card, as the kit's does. Unlike the kit's, something opened from it can hold it open, as a menu
//! opened from a button in it does while the pointer is in the menu, which is drawn outside the card, and one can show
//! while what it's for has the keyboard's focus too.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::{Popup, StyledExt as _, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Bounds, Context, DismissEvent, Div, ElementId, Entity, EntityId, EventEmitter,
    FocusHandle, Half as _, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled as _, Subscription, Task,
    TestSupportExt as _, WeakEntity, Window, canvas, div,
};

use crate::Space;

/// How long the pointer rests before a card shows: as long as GPUI waits before any other tooltip in the app shows,
/// so a card doesn't feel slower than one.
const OPENS_AFTER: Duration = Duration::from_millis(500);
/// A card goes as soon as the pointer does, as a tooltip does.
const CLOSES_AFTER: Duration = Duration::ZERO;
/// How long a card with a button in it waits after the pointer leaves what it opened from, so the pointer can cross the
/// gap to the button.
const CROSSING: Duration = Duration::from_millis(300);

/// A hover card with `id`, which shows once the pointer has rested on what it's for as long as before a tooltip, and
/// goes as soon as the pointer does. It draws no card of its own, so what it shows draws one, such as `look::raised`,
/// or nothing, as the update's details do, which show elsewhere. Where it opens is the caller's.
pub fn hover_card(id: impl Into<ElementId>) -> HoverCard {
    HoverCard::new(id, OPENS_AFTER, CLOSES_AFTER)
}

/// A [`hover_card`] with a button in it, which waits a moment after the pointer leaves what it's for, long enough for
/// the pointer to cross to the button.
pub fn hover_card_with_a_button(id: impl Into<ElementId>) -> HoverCard {
    HoverCard::new(id, OPENS_AFTER, CROSSING)
}

type Content = Box<dyn FnOnce(&HoldOpen, &mut Window, &mut App) -> AnyElement>;
type OpenChange = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// A card that shows over what it's for, its trigger, while the pointer is on either, or while something opened from
/// it holds it open, as [`hover_card`] makes one.
#[derive(IntoElement)]
pub struct HoverCard {
    id: ElementId,
    anchor: Anchor,
    trigger: Option<AnyElement>,
    trigger_style: StyleRefinement,
    content: Option<Content>,
    open_delay: Duration,
    close_delay: Duration,
    on_open_change: Option<OpenChange>,
    on_focus: Option<Focusable>,
    flips: bool,
}

/// What's said to assistive apps of what a card that shows on the keyboard's focus is for, since that's what they read
/// out as it takes the focus.
struct Focusable {
    role: Role,
    label: SharedString,
    said: SharedString,
}

impl HoverCard {
    pub(super) fn new(id: impl Into<ElementId>, open_delay: Duration, close_delay: Duration) -> Self {
        Self {
            id: id.into(),
            anchor: Anchor::TopCenter,
            trigger: None,
            trigger_style: StyleRefinement::default(),
            content: None,
            open_delay,
            close_delay,
            on_open_change: None,
            on_focus: None,
            flips: false,
        }
    }

    /// Which corner or side of the trigger the card opens from.
    #[must_use]
    pub fn anchor(mut self, anchor: impl Into<Anchor>) -> Self {
        self.anchor = anchor.into();
        self
    }

    /// Has it open on the other side of what it's for, over it rather than under it, or the other way round, while
    /// what it's for is in the half of the window that side faces, by where it was last drawn. A card is moved only to
    /// stay in the window, so one for something near the window's edge would otherwise cover what it's for, as a row's
    /// near the bottom would.
    #[must_use]
    pub fn flips_to_fit(mut self) -> Self {
        self.flips = true;
        self
    }

    #[must_use]
    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// How the box that holds the trigger, and that the card lines up with, is laid out.
    #[must_use]
    pub fn trigger_style(mut self, style: StyleRefinement) -> Self {
        self.trigger_style = style;
        self
    }

    /// What the card shows, made each time it's drawn open. It's given a [`HoldOpen`], for whatever in it opens
    /// something the card has to stay open for.
    #[must_use]
    pub fn content<E: IntoElement>(
        mut self,
        content: impl FnOnce(&HoldOpen, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.content = Some(Box::new(move |hold, window, cx| content(hold, window, cx).into_any_element()));
        self
    }

    /// Has it show while what it's for has the keyboard's focus too, which makes that one of Tab's stops: at once, since
    /// the focus was moved there on purpose, and for as long as it stays. A click that puts the focus there shows it no
    /// sooner than the pointer resting there does. What it's for is said to assistive apps as `role`, named `label`, with
    /// what the card says as `said`, whether it shows or not, since they can't see it.
    #[must_use]
    pub fn on_focus(mut self, role: Role, label: impl Into<SharedString>, said: impl Into<SharedString>) -> Self {
        self.on_focus = Some(Focusable { role, label: label.into(), said: said.into() });
        self
    }

    /// Called with whether it's open, each time that changes.
    #[must_use]
    pub fn on_open_change(mut self, changed: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(changed));
        self
    }
}

/// Holds a card open, whatever the pointer does, while something opened from it is, such as a menu opened from a
/// button in it. It's for the time the card was open when it was handed out, so one kept after the card has closed
/// does nothing, even once the card opens again.
#[derive(Clone)]
pub struct HoldOpen {
    shown: WeakEntity<Shown>,
    opening: usize,
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
struct Shown {
    open: bool,
    open_delay: Duration,
    close_delay: Duration,
    on_open_change: Option<OpenChange>,
    /// When it next opens or closes. A newer one replaces it, and dropping it cancels it.
    timer: Option<Task<()>>,
    /// Where the pointer and the keyboard's focus are, which keep it open.
    on: On,
    /// What takes the keyboard's focus for a card that shows on it, made the first time it's drawn so, with what says
    /// when the focus comes and goes.
    focus: Option<(FocusHandle, [Subscription; 2])>,
    /// Where what it's for was last drawn, for a card that flips to fit.
    trigger_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// What holds it open. It only closes once nothing does.
    holders: Vec<EntityId>,
    /// Counts each time it opens, so a hold handed out an earlier time does nothing.
    opening: usize,
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
    fn after(&mut self, delay: Duration, open: bool, window: &mut Window, cx: &mut Context<Self>) {
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
    fn stays(&self) -> bool {
        self.on.trigger || self.on.card || self.on.focus || !self.holders.is_empty()
    }

    /// Has it go after its delay, unless something keeps it open.
    fn close_unless_kept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.stays() {
            self.after(self.close_delay, false, window, cx);
        }
    }

    fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
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

    fn on_trigger(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.on.trigger = on;
        if on {
            self.after(self.open_delay, true, window, cx);
        } else {
            self.close_unless_kept(window, cx);
        }
    }

    fn on_card(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.on.card = on;
        if on {
            self.timer = None;
        } else {
            self.close_unless_kept(window, cx);
        }
    }

    /// What takes the keyboard's focus for it, made the first time it's asked for.
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) -> FocusHandle {
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

    fn focused(&mut self, focused: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.on.focus = focused;
        if focused {
            self.timer = None;
            self.set_open(true, window, cx);
        } else {
            self.close_unless_kept(window, cx);
        }
    }

    /// `holder` no longer holds it open, being dismissed or gone, if it still did.
    fn let_go(&mut self, holder: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let held = self.holders.len();
        self.holders.retain(|id| *id != holder);
        if self.holders.len() < held {
            self.close_unless_kept(window, cx);
        }
    }
}

impl RenderOnce for HoverCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (open_delay, close_delay, on_open_change) = (self.open_delay, self.close_delay, self.on_open_change);
        let shown = window.use_keyed_state(self.id.clone(), cx, |_, _| Shown::default());
        let focuses = self.on_focus.is_some();
        let (open, opening, focus, trigger_at) = shown.update(cx, |shown, cx| {
            (shown.open_delay, shown.close_delay, shown.on_open_change) = (open_delay, close_delay, on_open_change);
            (shown.open, shown.opening, focuses.then(|| shown.focus(window, cx)), shown.trigger_at.clone())
        });
        let anchor = if self.flips { fitting(self.anchor, trigger_at.get(), window) } else { self.anchor };
        let on_trigger = window.listener_for(&shown, |shown, on: &bool, window, cx| shown.on_trigger(*on, window, cx));
        // As big as the box `trigger_style` lays out, so what it's for can fill that, as a tab fills its bar's height.
        let trigger = div().id("trigger").size_full().children(self.trigger).on_hover(on_trigger);
        // Measured only for a card that flips, so every other is laid out as it was.
        let trigger = if self.flips { trigger.relative().child(measuring(trigger_at)) } else { trigger };
        let trigger = match (focus, self.on_focus) {
            (Some(focus), Some(said)) => trigger
                .role(said.role)
                .aria_label(said.label)
                .aria_description(said.said)
                .test_support()
                .track_focus(&focus)
                .into_any_element(),
            _ => trigger.into_any_element(),
        };
        let popup = Popup::new(self.id, trigger).refine_style(&self.trigger_style).anchor(anchor);
        let Some(content) = self.content.filter(|_| open) else { return popup };
        let on_card = window.listener_for(&shown, |shown, on: &bool, window, cx| shown.on_card(*on, window, cx));
        let hold = HoldOpen { shown: shown.downgrade(), opening };
        popup.content(around_content(anchor).on_hover(on_card).child(content(&hold, window, cx)))
    }
}

/// `anchor`, or the one on the other side of what the card is for, at the same corner or the middle, while what it's
/// for, last drawn `at`, is in the half of the window that `anchor` opens the card towards.
fn fitting(anchor: Anchor, at: Option<Bounds<Pixels>>, window: &Window) -> Anchor {
    let Some(at) = at else { return anchor };
    let lower = at.center().y > window.viewport_size().height.half();
    match (anchor, lower) {
        (Anchor::TopLeft, true) => Anchor::BottomLeft,
        (Anchor::TopCenter, true) => Anchor::BottomCenter,
        (Anchor::TopRight, true) => Anchor::BottomRight,
        (Anchor::BottomLeft, false) => Anchor::TopLeft,
        (Anchor::BottomCenter, false) => Anchor::TopCenter,
        (Anchor::BottomRight, false) => Anchor::TopRight,
        (anchor, _) => anchor,
    }
}

/// Keeps where what a card is for is drawn, `at`, laid over it, so it takes no room.
fn measuring(at: Rc<Cell<Option<Bounds<Pixels>>>>) -> impl IntoElement {
    canvas(move |bounds, _, _| at.set(Some(bounds)), |_, (), _, _| {}).absolute().top_0().left_0().size_full()
}

/// The box around what the card shows: a little way off what it's for, as the kit's popovers are, and in front of
/// what's under it, so the pointer on the card isn't on that too.
fn around_content(anchor: Anchor) -> gpui_kit::Stateful<Div> {
    let around = v_flex().id("content").occlude().tab_group();
    match anchor {
        Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight => around.bottom(Space::XSmall),
        Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight | Anchor::LeftCenter | Anchor::RightCenter => {
            around.top(Space::XSmall)
        }
    }
}
