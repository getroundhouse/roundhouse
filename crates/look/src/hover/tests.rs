//! When the app's hover card shows and goes, by the pointer and the keyboard's focus, and while something opened from
//! it holds it open.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{
    AppContext as _, Context, DismissEvent, Entity, EventEmitter, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Render, Role, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::{HoldOpen, HoverCard};

const OPENS: Duration = Duration::from_millis(100);
const CLOSES: Duration = Duration::from_millis(50);

/// A 20 px square 200 px in from the window's corner, so the pointer can be off it on every side, with a card over it
/// that holds a 10 px square. It keeps the card's hold, as a menu opened from the card would. Its card shows on the
/// keyboard's focus too when it `focuses`.
struct Harness {
    hold: Rc<RefCell<Option<HoldOpen>>>,
    changes: Rc<RefCell<Vec<bool>>>,
    focuses: bool,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (kept, changes) = (self.hold.clone(), self.changes.clone());
        let card = HoverCard::new("card", OPENS, CLOSES)
            .anchor(gpui_kit::Anchor::TopLeft)
            .on_open_change(move |open, _, _| changes.borrow_mut().push(*open))
            .trigger(div().debug_selector(|| "trigger".into()).size(px(20.)))
            .content(move |hold, _, _| {
                *kept.borrow_mut() = Some(hold.clone());
                div().debug_selector(|| "card".into()).size(px(10.))
            });
        let card = if self.focuses { card.on_focus(Role::Group, "Square", "A card") } else { card };
        div().p(px(200.)).child(card)
    }
}

/// What holds a card open in these tests, as a menu does, until it's dismissed or dropped.
struct Menu;

impl EventEmitter<DismissEvent> for Menu {}

type Kept<T> = Rc<RefCell<T>>;

fn harness(cx: &mut TestAppContext) -> (&mut VisualTestContext, Kept<Option<HoldOpen>>, Kept<Vec<bool>>) {
    harness_that(cx, false)
}

fn harness_that(
    cx: &mut TestAppContext,
    focuses: bool,
) -> (&mut VisualTestContext, Kept<Option<HoldOpen>>, Kept<Vec<bool>>) {
    let (hold, changes) = (Rc::default(), Rc::default());
    let made = Harness { hold: Rc::clone(&hold), changes: Rc::clone(&changes), focuses };
    let (_, cx) = cx.add_window_view(move |_, _| made);
    cx.update(|window, cx| {
        // In front, since the focus is only said to come or go in the window in front.
        if focuses {
            window.activate_window();
        }
        window.draw(cx).clear(cx);
    });
    (cx, hold, changes)
}

/// Moves the keyboard's focus on to the next of Tab's stops, as a press of Tab does, after a key, which tells the
/// window the keyboard is in use.
fn tab(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("a");
    cx.update(Window::focus_next);
    settle(cx, Duration::ZERO);
}

/// Lets `after` go by, and draws what that left.
fn settle(cx: &mut VisualTestContext, after: Duration) {
    cx.executor().advance_clock(after);
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.draw(cx).clear(cx);
    });
}

/// Moves the pointer to `(x, y)`, then lets `after` go by.
fn move_to(cx: &mut VisualTestContext, x: f32, y: f32, after: Duration) {
    cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::default());
    settle(cx, after);
}

fn showing(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("card").is_some()
}

/// Opens the card, and holds it open with a menu of its own.
fn held_open(cx: &mut VisualTestContext, hold: &Kept<Option<HoldOpen>>) -> Entity<Menu> {
    move_to(cx, 210., 210., OPENS);
    let hold = hold.borrow().clone().expect("the card was drawn, with its hold");
    let menu = cx.update(|_, cx| cx.new(|_| Menu));
    cx.update(|window, cx| hold.while_open(&menu, window, cx));
    menu
}

fn dismiss(cx: &mut VisualTestContext, menu: &Entity<Menu>) {
    cx.update(|_, cx| menu.update(cx, |_, cx| cx.emit(DismissEvent)));
}

#[gpui_kit::test]
fn a_card_shows_once_the_pointer_has_rested_on_its_trigger_and_goes_once_it_has_left_both(cx: &mut TestAppContext) {
    let (cx, _, changes) = harness(cx);

    move_to(cx, 210., 210., OPENS / 2);
    assert!(!showing(cx), "not before it has rested");
    move_to(cx, 211., 211., OPENS);
    assert!(showing(cx));
    let card = cx.debug_bounds("card").unwrap().center();
    move_to(cx, card.x.as_f32(), card.y.as_f32(), CLOSES * 2);
    assert!(showing(cx), "the pointer crossed to the card");
    move_to(cx, 210., 210., OPENS);
    assert!(showing(cx), "and back");
    move_to(cx, 5., 5., CLOSES);

    assert!(!showing(cx));
    assert_eq!(*changes.borrow(), [true, false], "it said it opened once, though the pointer came back to the trigger");
}

#[gpui_kit::test]
fn a_card_whose_pointer_left_before_its_delay_doesnt_show(cx: &mut TestAppContext) {
    let (cx, _, changes) = harness(cx);

    move_to(cx, 210., 210., OPENS / 2);
    move_to(cx, 5., 5., OPENS * 4);

    assert!(!showing(cx));
    assert!(changes.borrow().is_empty());
}

#[gpui_kit::test]
fn a_card_held_open_by_a_menu_stays_wherever_the_pointer_goes_until_the_menu_is_dismissed(cx: &mut TestAppContext) {
    let (cx, hold, changes) = harness(cx);
    let menu = held_open(cx, &hold);

    move_to(cx, 5., 5., CLOSES * 4);
    let stayed = showing(cx);
    dismiss(cx, &menu);
    settle(cx, CLOSES);

    assert!(stayed, "held, though the pointer left both");
    assert!(!showing(cx), "and gone once the menu was, with the pointer on neither");
    assert_eq!(*changes.borrow(), [true, false]);
}

#[gpui_kit::test]
fn a_card_held_open_by_a_menu_that_goes_without_being_dismissed_goes_too(cx: &mut TestAppContext) {
    let (cx, hold, _) = harness(cx);
    let menu = held_open(cx, &hold);
    move_to(cx, 5., 5., CLOSES * 4);
    let stayed = showing(cx);

    drop(menu);
    cx.update(|_, _| {});
    settle(cx, CLOSES);

    assert!(stayed);
    assert!(!showing(cx));
}

#[gpui_kit::test]
fn a_card_stays_while_anything_holds_it(cx: &mut TestAppContext) {
    let (cx, hold, _) = harness(cx);
    let first = held_open(cx, &hold);
    let second = cx.update(|_, cx| cx.new(|_| Menu));
    let hold = hold.borrow().clone().unwrap();
    cx.update(|window, cx| hold.while_open(&second, window, cx));
    move_to(cx, 5., 5., CLOSES * 4);

    // As a menu is, dismissed, then dropped a frame later, which has to let go of nothing the other holds.
    dismiss(cx, &first);
    drop(first);
    cx.update(|_, _| {});
    settle(cx, CLOSES * 4);
    let still_held = showing(cx);
    dismiss(cx, &second);
    settle(cx, CLOSES);

    assert!(still_held, "the second menu still held it");
    assert!(!showing(cx));
}

#[gpui_kit::test]
fn a_card_let_go_of_with_the_pointer_on_it_or_what_its_for_stays(cx: &mut TestAppContext) {
    let (cx, hold, _) = harness(cx);
    let menu = held_open(cx, &hold);
    let card = cx.debug_bounds("card").unwrap().center();
    move_to(cx, card.x.as_f32(), card.y.as_f32(), CLOSES);
    dismiss(cx, &menu);
    settle(cx, CLOSES * 4);
    let stayed_on_the_card = showing(cx);

    let menu = held_open(cx, &hold);
    dismiss(cx, &menu);
    settle(cx, CLOSES * 4);

    assert!(stayed_on_the_card);
    assert!(showing(cx), "on what it's for");
}

#[gpui_kit::test]
fn a_hold_kept_after_its_card_closed_doesnt_hold_it_once_it_opens_again(cx: &mut TestAppContext) {
    let (cx, hold, _) = harness(cx);
    move_to(cx, 210., 210., OPENS);
    let kept = hold.borrow().clone().unwrap();
    move_to(cx, 5., 5., CLOSES);
    let menu = cx.update(|_, cx| cx.new(|_| Menu));
    cx.update(|window, cx| kept.while_open(&menu, window, cx));
    move_to(cx, 210., 210., OPENS);
    let opened_again = showing(cx);

    cx.update(|window, cx| kept.while_open(&menu, window, cx));
    move_to(cx, 5., 5., CLOSES * 4);

    assert!(opened_again);
    assert!(!showing(cx));
}

#[gpui_kit::test]
fn a_card_that_shows_on_focus_shows_at_once_as_the_keyboard_moves_the_focus_there_and_goes_as_it_moves_off(
    cx: &mut TestAppContext,
) {
    let (cx, _, changes) = harness_that(cx, true);

    tab(cx);
    let shown = showing(cx);
    settle(cx, CLOSES * 4);
    let stayed = showing(cx);
    cx.update(Window::blur);
    settle(cx, CLOSES);

    assert!(shown, "with no wait, though the pointer is nowhere near");
    assert!(stayed, "for as long as the focus is there");
    assert!(!showing(cx));
    assert_eq!(*changes.borrow(), [true, false]);
}

#[gpui_kit::test]
fn a_card_whose_trigger_a_click_focused_shows_and_goes_by_the_pointer(cx: &mut TestAppContext) {
    let (cx, _, _) = harness_that(cx, true);

    cx.simulate_click(point(px(210.), px(210.)), Modifiers::default());
    settle(cx, OPENS / 2);
    let at_once = showing(cx);
    settle(cx, OPENS);
    let once_rested = showing(cx);
    move_to(cx, 5., 5., CLOSES);

    assert!(!at_once, "not before the pointer has rested");
    assert!(once_rested);
    assert!(!showing(cx), "gone with the pointer, though the focus stayed");
}

/// A 20 px square with a card under it that flips to fit, in the top or the bottom of the window as `low` says.
struct Placed {
    low: bool,
}

impl Render for Placed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let card = HoverCard::new("card", OPENS, CLOSES)
            .anchor(gpui_kit::Anchor::TopLeft)
            .flips_to_fit()
            .trigger(div().debug_selector(|| "trigger".into()).size(px(20.)))
            .content(|_, _, _| div().debug_selector(|| "card".into()).size(px(10.)));
        let window = div().size_full().flex().flex_col().p(px(100.));
        if self.low { window.justify_end() } else { window.justify_start() }.child(card)
    }
}

/// Whether the card over a square, in the top or the bottom of the window as `low` says, opens under it.
fn opens_under(cx: &mut TestAppContext, low: bool) -> bool {
    let (_, cx) = cx.add_window_view(move |_, _| Placed { low });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let square = cx.debug_bounds("trigger").unwrap();
    move_to(cx, square.center().x.as_f32(), square.center().y.as_f32(), OPENS);
    let card = cx.debug_bounds("card").expect("the card shows");
    assert!(card.top() >= square.bottom() || card.bottom() <= square.top(), "{card:?} covers {square:?}");
    card.top() >= square.bottom()
}

#[gpui_kit::test]
fn a_card_that_flips_to_fit_opens_over_what_its_for_near_the_windows_bottom_and_under_it_otherwise(
    cx: &mut TestAppContext,
) {
    assert!(opens_under(cx, false));
    assert!(!opens_under(cx, true));
}

#[gpui_kit::test]
fn a_card_shows_on_focus_only_when_asked_to(cx: &mut TestAppContext) {
    let (cx, _, _) = harness(cx);
    cx.update(|window, _| window.activate_window());

    tab(cx);

    assert!(!showing(cx), "what it's for isn't one of Tab's stops");
}
