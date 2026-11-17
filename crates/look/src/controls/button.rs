//! The app's button: made of a kind and a size, with what it shows, what it does and what state it's in, and nothing
//! else. It holds the kit's button and draws with it, but gives none of the kit's or GPUI's styling to whoever makes
//! it, so every button of a kind and a size looks the same. `docs/component-library.md` says why, and how a component's
//! API is laid out.

use std::rc::Rc;

use gpui_kit::component::button::{Button as KitButton, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu;
use gpui_kit::component::{Disableable as _, Icon, Selectable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, Anchor, App, ClickEvent, ElementId, InteractiveElement, Interactivity, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, RenderOnce, SharedString, StyleRefinement, Styled, Window,
};

use super::{ButtonKind, Sizing as _};
use crate::ControlSize;
use crate::colors::colors;
use crate::hand::Hand;
use crate::work::Work;

/// A button with words, and maybe an icon beside them, of `kind`, as big as `size` has it.
pub fn button(id: impl Into<ElementId>, kind: ButtonKind, size: ControlSize, words: impl Into<SharedString>) -> Button {
    Button::new(kind.drawn(KitButton::new(id)).sized(size).label(words), kind, Shape::Words(size))
}

/// A button that's only an icon, of `kind`, as a square of `size`.
pub fn icon_button(id: impl Into<ElementId>, kind: ButtonKind, size: ControlSize, icon: impl Into<Icon>) -> Button {
    Button::new(kind.drawn(KitButton::new(id)).sized_square(size).icon(icon.into()), kind, Shape::Square(size))
}

/// A square of `kind` and `size` with something drawn in it that isn't an icon, such as Stop's filled square, in the
/// middle. It's named by [`Button::accessibility_label`], since it has no words.
pub fn glyph_button(id: impl Into<ElementId>, kind: ButtonKind, size: ControlSize, glyph: impl IntoElement) -> Button {
    Button::new(kind.drawn(KitButton::new(id)).sized_square(size).child(glyph), kind, Shape::Square(size))
}

/// A side of a button, which it can stand out past.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// Whether a button has words, and how far in they are from its edge, or is a square, and how far in its icon is.
#[derive(Clone, Copy)]
enum Shape {
    Words(ControlSize),
    Square(ControlSize),
}

impl Shape {
    /// How far its words or icon are in from its edge.
    fn room(self) -> Pixels {
        match self {
            Shape::Words(size) => size.padding(),
            Shape::Square(size) => size.inset(),
        }
    }
}

/// A button, made with [`button`], [`icon_button`] or [`glyph_button`].
#[derive(IntoElement)]
#[must_use]
pub struct Button {
    kit: KitButton,
    kind: ButtonKind,
    shape: Shape,
    off: bool,
    waiting: bool,
    work: Option<Work>,
}

impl Button {
    fn new(kit: KitButton, kind: ButtonKind, shape: Shape) -> Self {
        Button { kit, kind, shape, off: false, waiting: false, work: None }
    }

    fn kit(mut self, change: impl FnOnce(KitButton) -> KitButton) -> Self {
        self.kit = change(self.kit);
        self
    }

    /// An icon beside its words, before them.
    pub fn icon(self, icon: impl Into<Icon>) -> Self {
        self.kit(|kit| kit.icon(icon.into()))
    }

    /// Something after its words, such as a detail in muted text, an icon that says where it goes, or the key that
    /// does the same.
    pub fn after(self, element: impl IntoElement) -> Self {
        self.kit(|kit| kit.child(element))
    }

    /// What it says it does while the pointer rests on it.
    pub fn tooltip(self, tooltip: impl Into<SharedString>) -> Self {
        self.kit(|kit| kit.tooltip(tooltip))
    }

    /// What it says it does while the pointer rests on it, with the keys that do the same.
    pub fn tooltip_with_action(
        self,
        tooltip: impl Into<SharedString>,
        action: &dyn Action,
        context: Option<&str>,
    ) -> Self {
        self.kit(|kit| kit.tooltip_with_action(tooltip, action, context))
    }

    /// What an assistive app calls it, where its words don't say, as for a button that's only an icon.
    pub fn accessibility_label(self, label: impl Into<SharedString>) -> Self {
        self.kit(|kit| kit.accessibility_label(label))
    }

    /// What a click does, or Enter or Space once it has the keyboard.
    pub fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.kit(|kit| kit.on_click(handler))
    }

    /// What a press does, as it goes down, before it's let go, such as opening the menus as a menu bar does.
    pub fn on_press(self, handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static) -> Self {
        self.kit(|kit| kit.on_mouse_down(MouseButton::Left, handler))
    }

    /// Keeps a press to itself, for a button in something that takes presses too, such as a row's, so pressing the
    /// button doesn't also press the row.
    pub fn keeps_its_press(self) -> Self {
        self.on_press(|_, _, cx| cx.stop_propagation())
    }

    /// On, as a view that's chosen is, or as the button that opens something is while it's open.
    pub fn on(self, on: bool) -> Self {
        self.kit(|kit| kit.selected(on))
    }

    /// Greyed out, under the arrow, and pressing it does nothing.
    pub fn off(mut self, off: bool) -> Self {
        self.off = off;
        self
    }

    /// Waiting on what it started, with a spinner for its icon, under the arrow, and pressing it does nothing.
    pub fn waiting(mut self, waiting: bool) -> Self {
        self.waiting = waiting;
        self
    }

    /// As wide as `width`, rather than its words.
    pub fn width(self, width: Pixels) -> Self {
        self.kit(|kit| kit.w(width))
    }

    /// In the colours of where a pull request stands, as the pull request's bar draws its buttons: a main action filled
    /// with its colour, where it has one of its own, and an outlined one edged and written in it.
    pub fn of_work(mut self, work: Work) -> Self {
        self.work = Some(work);
        self
    }

    /// Takes an even share of the room across what it's in with the others that grow, as the update card's two answers
    /// do.
    pub fn grows(self) -> Self {
        self.kit(Styled::flex_1)
    }

    /// As wide as what it's in, as the update card's own button is.
    pub fn full_width(self) -> Self {
        self.kit(Styled::w_full)
    }

    /// Stands out past its own room on each of `sides`, so its words or its icon line up with text or marks beside it
    /// or under it, rather than that room in from them, as `docs/spec/components.md` says. Its fill, as the pointer
    /// lights it, then reaches past them by that room.
    pub fn stands_out(self, sides: &[Side]) -> Self {
        let out = -self.shape.room();
        let sides = sides.to_vec();
        self.kit(|kit| {
            sides.iter().fold(kit, |kit, side| match side {
                Side::Top => kit.mt(out),
                Side::Right => kit.mr(out),
                Side::Bottom => kit.mb(out),
                Side::Left => kit.ml(out),
            })
        })
    }

    /// Hidden but while the pointer is over `group`, which keeps its room, such as a heading's button that's there only
    /// while you're in the heading.
    pub fn shown_while_over(self, group: impl Into<SharedString>) -> Self {
        let group = group.into();
        self.kit(|kit| kit.invisible().group_hover(group, Styled::visible))
    }

    /// Opens a menu, built by `build` each time it opens, from its `anchor` corner. A button with words shows that it
    /// opens a menu of choices with a caret after them, and is on while the menu is open.
    pub fn menu(
        self,
        anchor: Anchor,
        build: impl Fn(crate::Menu, &mut Window, &mut App) -> crate::Menu + 'static,
    ) -> MenuButton {
        let caret = matches!(self.shape, Shape::Words(_));
        let trigger = Trigger(self.kit(|kit| kit.dropdown_caret(caret)));
        MenuButton { trigger, anchor, build: Rc::new(build), on_open_change: None }
    }

    /// The kit's button, as it's drawn now: its kind's words, and the hand while pressing it does something.
    pub(super) fn drawn(self, cx: &App) -> KitButton {
        let pressable = !self.off && !self.waiting;
        let colors = colors(cx);
        let words = self.kind.words(colors);
        let (off, kind) = (self.off, self.kind);
        let kit = match self.work {
            Some(work) => match (kind, work.hue(colors)) {
                // The kit draws a custom button's colour faintly, so the fill is set on the button itself, which hover
                // and press still change. Not while it's off, since the kit lays the button's own style over its
                // greyed-out one, which would keep the fill.
                (ButtonKind::Main, Some(fill)) => {
                    let style = ButtonCustomVariant::new(cx)
                        .color(fill)
                        .foreground(colors.on_main)
                        .hover(fill.opacity(0.85))
                        .active(fill.opacity(0.7));
                    self.kit.custom(style).when(!off, |kit| kit.bg(fill))
                }
                (ButtonKind::Outlined, _) => self.kit.border_color(work.edge(colors)).text_color(work.text(colors)),
                _ => self.kit,
            },
            None => self.kit,
        };
        kit.disabled(self.off)
            .loading(self.waiting)
            .when(pressable, Hand::hand)
            // Not while it's off, since the kit lays a button's own style over its greyed-out one.
            .when_some(words.filter(|_| !self.off), Styled::text_color)
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.drawn(cx)
    }
}

/// So a button can open what the kit opens from something, such as a popover, which marks it on while it's open.
impl Selectable for Button {
    fn selected(self, selected: bool) -> Self {
        self.on(selected)
    }

    fn is_selected(&self) -> bool {
        self.kit.is_selected()
    }

    fn open(self, open: bool) -> Self {
        self.kit(|kit| kit.open(open))
    }

    fn is_open(&self) -> bool {
        self.kit.is_open()
    }
}

/// How a menu is built each time it opens.
type Build = Rc<dyn Fn(crate::Menu, &mut Window, &mut App) -> crate::Menu>;

/// What's done as a menu opens or closes, told whether it's open now.
type OpenChange = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// A button that opens a menu, made with [`Button::menu`].
#[derive(IntoElement)]
#[must_use]
pub struct MenuButton {
    trigger: Trigger,
    anchor: Anchor,
    build: Build,
    on_open_change: Option<OpenChange>,
}

impl MenuButton {
    /// What's done as the menu opens or closes, told whether it's open now.
    pub fn on_open_change(mut self, change: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(change));
        self
    }
}

impl RenderOnce for MenuButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let build = self.build;
        let menu = self
            .trigger
            .dropdown_menu_with_anchor(self.anchor, crate::menu(move |menu, window, cx| build(menu, window, cx)));
        menu.when_some(self.on_open_change, |menu, change| {
            menu.on_open_change(move |open, window, cx| change(open, window, cx))
        })
    }
}

/// The button a menu opens from, which the kit's menu needs to be able to style, to mark it on while the menu is open.
/// It's no one's but [`MenuButton`]'s, so nothing else styles a button.
#[derive(IntoElement)]
struct Trigger(Button);

impl RenderOnce for Trigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.0.drawn(cx)
    }
}

impl Selectable for Trigger {
    fn selected(self, selected: bool) -> Self {
        Trigger(self.0.selected(selected))
    }

    fn is_selected(&self) -> bool {
        self.0.is_selected()
    }

    fn open(self, open: bool) -> Self {
        Trigger(self.0.open(open))
    }

    fn is_open(&self) -> bool {
        self.0.is_open()
    }
}

impl DropdownMenu for Trigger {}

impl Styled for Trigger {
    fn style(&mut self) -> &mut StyleRefinement {
        self.0.kit.style()
    }
}

impl InteractiveElement for Trigger {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.0.kit.interactivity()
    }
}
