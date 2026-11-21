//! The app's hover card: it shows once the pointer has rested on what it's for, and goes once the pointer has left
//! both that and the card, as the kit's does. Under touch, where nothing rests, a tap on what it's for shows it, and a
//! tap anywhere but the card puts it away, as `components.md` says. Unlike the kit's, something opened from it can hold it open, as a menu
//! opened from a button in it does while the pointer is in the menu, which is drawn outside the card, and one can show
//! while what it's for has the keyboard's focus too.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::{Popup, StyledExt as _, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Bounds, ClickEvent, Div, ElementId, Half as _, InteractiveElement as _, IntoElement,
    MouseDownEvent, ParentElement as _, Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled as _, TestSupportExt as _, Window, canvas, div,
};

use super::shown::{HoldOpen, Shown};
use crate::{Input, Space};

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
pub(super) type OpenChange = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

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
        let touch = Input::of(cx) == Input::Touch;
        // As big as the box `trigger_style` lays out, so what it's for can fill that, as a tab fills its bar's height.
        let trigger = div().id("trigger").size_full().children(self.trigger);
        let trigger = if touch {
            trigger.on_click(window.listener_for(&shown, |shown, _: &ClickEvent, window, cx| shown.tapped(window, cx)))
        } else {
            trigger
                .on_hover(window.listener_for(&shown, |shown, on: &bool, window, cx| shown.on_trigger(*on, window, cx)))
        };
        // Measured only for a card that flips, or under touch, to tell a tap on it, so every other is laid out as it
        // was.
        let trigger = if self.flips || touch { trigger.relative().child(measuring(trigger_at)) } else { trigger };
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
        let around = around_content(anchor).on_hover(on_card);
        let around = if touch {
            around.on_mouse_down_out(window.listener_for(&shown, |shown, event: &MouseDownEvent, window, cx| {
                shown.pressed_off_the_card(event.position, window, cx);
            }))
        } else {
            around
        };
        popup.content(around.child(content(&hold, window, cx)))
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
