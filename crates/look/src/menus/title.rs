//! A bar's title that opens a menu under it, such as a workspace's chats, which the kit's dropdown opens from, as it
//! does from a button. The kit marks it on while the menu is open, and it shows that as it shows a press.

use gpui_kit::base::ObservedElement;
use gpui_kit::component::Selectable;
use gpui_kit::component::menu::DropdownMenu;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, Div, Hsla, InteractiveElement, Interactivity, IntoElement, RenderOnce, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
};

use super::Menu;

/// The title `drawn`, which opens the menu `build` makes each time, under it, from its middle, as a phone's own title
/// menus open, filled with `pressed` while it's pressed and while the menu is open.
pub(crate) fn under_the_title(
    drawn: ObservedElement<Stateful<Div>>,
    pressed: Hsla,
    build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static,
) -> impl IntoElement {
    Title { drawn, open: false, pressed }.dropdown_menu_with_anchor(Anchor::TopCenter, super::menu(build))
}

/// The title, and whether its menu is open.
#[derive(IntoElement)]
struct Title {
    drawn: ObservedElement<Stateful<Div>>,
    open: bool,
    /// What it's filled with while it's pressed, and while its menu is open.
    pressed: Hsla,
}

impl RenderOnce for Title {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let pressed = self.pressed;
        self.drawn.when(self.open, |title| title.bg(pressed)).active(move |title| title.bg(pressed))
    }
}

impl Selectable for Title {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.open
    }
}

impl DropdownMenu for Title {}

impl Styled for Title {
    fn style(&mut self) -> &mut StyleRefinement {
        self.drawn.style()
    }
}

impl InteractiveElement for Title {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.drawn.interactivity()
    }
}
