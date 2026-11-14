//! The app's hover card: it shows once the pointer has rested on what it's for, and goes once the pointer has left
//! both that and the card, as the kit's does. Unlike the kit's, something opened from it can hold it open, as a menu
//! opened from a button in it does while the pointer is in the menu, which is drawn outside the card.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::{Popup, StyledExt as _, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Context, DismissEvent, Div, ElementId, Entity, EntityId, EventEmitter,
    InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, StatefulInteractiveElement as _,
    StyleRefinement, Styled as _, Task, WeakEntity, Window, div,
};

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
        }
    }

    /// Which corner or side of the trigger the card opens from.
    #[must_use]
    pub fn anchor(mut self, anchor: impl Into<Anchor>) -> Self {
        self.anchor = anchor.into();
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
    on_trigger: bool,
    on_card: bool,
    /// What holds it open. It only closes once nothing does.
    holders: Vec<EntityId>,
    /// Counts each time it opens, so a hold handed out an earlier time does nothing.
    opening: usize,
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

    /// Whether something keeps it open: the pointer, on the card or what it's for, or what holds it.
    fn stays(&self) -> bool {
        self.on_trigger || self.on_card || !self.holders.is_empty()
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
        self.on_trigger = on;
        if on {
            self.after(self.open_delay, true, window, cx);
        } else {
            self.close_unless_kept(window, cx);
        }
    }

    fn on_card(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.on_card = on;
        if on {
            self.timer = None;
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
        let (open, opening) = shown.update(cx, |shown, _| {
            (shown.open_delay, shown.close_delay, shown.on_open_change) = (open_delay, close_delay, on_open_change);
            (shown.open, shown.opening)
        });
        let on_trigger = window.listener_for(&shown, |shown, on: &bool, window, cx| shown.on_trigger(*on, window, cx));
        let trigger = div().id("trigger").children(self.trigger).on_hover(on_trigger);
        let popup = Popup::new(self.id, trigger).refine_style(&self.trigger_style).anchor(self.anchor);
        let Some(content) = self.content.filter(|_| open) else { return popup };
        let on_card = window.listener_for(&shown, |shown, on: &bool, window, cx| shown.on_card(*on, window, cx));
        let hold = HoldOpen { shown: shown.downgrade(), opening };
        popup.content(around_content(self.anchor).on_hover(on_card).child(content(&hold, window, cx)))
    }
}

/// The box around what the card shows: a little way off what it's for, as the kit's popovers are, and in front of
/// what's under it, so the pointer on the card isn't on that too.
fn around_content(anchor: Anchor) -> gpui_kit::Stateful<Div> {
    let around = v_flex().id("content").occlude().tab_group();
    match anchor {
        Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight => around.bottom_1(),
        Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight | Anchor::LeftCenter | Anchor::RightCenter => {
            around.top_1()
        }
    }
}
