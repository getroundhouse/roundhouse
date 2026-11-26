use std::rc::Rc;

use gpui_kit::base::A11yDisabledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    MouseUpEvent, ParentElement, Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};

use crate::colors::colors;
use crate::hand::Hand as _;
use crate::heights::RowSize;
use crate::menus::{Menu, RightClickMenu as _};
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::theme::{CHOSEN, PRESSED, UNDER_POINTER};
use crate::tooltips::Tooltipped as _;
use crate::typography::{Type, Typeset as _};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex};

use crate::icon_sizes::{IconSize, IconSizing as _};

/// How far in from its list's edge a row sits, on every side of the list, so its rounded corners don't meet the edge.
pub const ROWS_INSET: Space = Space::XSmall;

/// A row of `size`, found by `id`, with nothing in it yet.
pub fn list_row(id: impl Into<ElementId>, size: RowSize) -> ListRow {
    ListRow {
        id: id.into(),
        size,
        chosen: false,
        off: false,
        wraps: false,
        indent: px(0.),
        end: End::Nothing,
        role: None,
        tooltip: None,
        on_click: None,
        on_hover: None,
        on_press: None,
        on_let_go: None,
        on_let_go_outside: None,
        menu: None,
        children: Vec::new(),
    }
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type OnHover = Box<dyn Fn(&bool, &mut Window, &mut App)>;
type OnPress = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;
type OnLetGo = Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App)>;
type BuildMenu = Rc<dyn Fn(Menu, &mut Window, &mut App) -> Menu>;

/// What a row draws at its end, after what it holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Nothing,
    /// A chevron, which says a press shows what the row names in the list's place.
    Chevron,
}

/// One of what a list holds, which [`list_row`] makes. What's put in it is laid out in a line, from its start.
#[derive(IntoElement)]
#[must_use]
pub struct ListRow {
    id: ElementId,
    size: RowSize,
    chosen: bool,
    off: bool,
    wraps: bool,
    indent: Pixels,
    end: End,
    role: Option<(Role, SharedString)>,
    tooltip: Option<SharedString>,
    on_click: Option<OnClick>,
    on_hover: Option<OnHover>,
    on_press: Option<OnPress>,
    on_let_go: Option<OnLetGo>,
    on_let_go_outside: Option<OnLetGo>,
    menu: Option<BuildMenu>,
    children: Vec<AnyElement>,
}

impl ListRow {
    /// Washed as what's chosen, such as the file showing or the page of Settings that's open.
    pub fn chosen(mut self, chosen: bool) -> Self {
        self.chosen = chosen;
        self
    }

    /// Can't be pressed for now, such as Create while there's no project to create in: it isn't washed under the
    /// pointer, has the arrow, a click on it does nothing, and assistive apps say it's off.
    pub fn off(mut self, off: bool) -> Self {
        self.off = off;
        self
    }

    /// As tall as what it holds, at least its size's height, with its room above and below what it holds as well as
    /// at its ends, and what it holds at its top, such as a prompt of two lines in the outline.
    pub fn wraps(mut self) -> Self {
        self.wraps = true;
        self
    }

    /// Set in by `indent` after its start, such as a file in a folder.
    pub fn indent(mut self, indent: Pixels) -> Self {
        self.indent = indent;
        self
    }

    /// A chevron at its end, which says a press shows what it names in the list's place, as on a handheld, where a
    /// row is the way to another screen. What it holds before that takes the room that's left.
    pub fn opens(mut self) -> Self {
        self.end = End::Chevron;
        self
    }

    /// One option of a list to choose from, called `label` to assistive apps, which say whether it's chosen, such as a
    /// page of Settings.
    pub fn listed(mut self, label: impl Into<SharedString>) -> Self {
        self.role = Some((Role::ListBoxOption, label.into()));
        self
    }

    /// A link, called `label` to assistive apps, such as a page a web search found.
    pub fn link(mut self, label: impl Into<SharedString>) -> Self {
        self.role = Some((Role::Link, label.into()));
        self
    }

    /// A button, called `label` to assistive apps, such as a workspace in the sidebar, which opens it.
    pub fn button(mut self, label: impl Into<SharedString>) -> Self {
        self.role = Some((Role::Button, label.into()));
        self
    }

    /// What it names, while the pointer rests on it, where its words are cut short.
    pub fn tooltip(mut self, words: impl Into<SharedString>) -> Self {
        self.tooltip = Some(words.into());
        self
    }

    /// What a click on it does.
    pub fn on_click(mut self, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(on_click));
        self
    }

    /// What the pointer coming onto it, `true`, and leaving it, `false`, does, such as showing a button that's there
    /// only under the pointer.
    pub fn on_hover(mut self, on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Box::new(on_hover));
        self
    }

    /// What a left press on it does as it goes down, before it's let go.
    pub fn on_press(mut self, on_press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Box::new(on_press));
        self
    }

    /// What letting a left press go over it does, wherever the press started.
    pub fn on_let_go(mut self, on_let_go: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_let_go = Some(Box::new(on_let_go));
        self
    }

    /// What letting a left press go anywhere but over it does, such as forgetting a press that started on it.
    pub fn on_let_go_outside(mut self, on_let_go: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_let_go_outside = Some(Box::new(on_let_go));
        self
    }

    /// The menu `build` fills, where it's right-clicked.
    pub fn right_click_menu(mut self, build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static) -> Self {
        self.menu = Some(Rc::new(build));
        self
    }
}

impl ParentElement for ListRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The room at a row's ends and between what's in it, by how tall it is: a little more in a large row, which is a
/// table's, so its parts read apart across a wide pane.
fn room(size: RowSize) -> Space {
    match size {
        RowSize::Small | RowSize::Regular => Space::Small,
        RowSize::Large => Space::Medium,
    }
}

impl RenderOnce for ListRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let room = room(self.size);
        let (on, clickable) = (!self.off, self.on_click.is_some() && !self.off);
        let drawn = h_flex()
            .id(self.id)
            .test_support()
            .relative()
            .flex_none()
            .w_full()
            .map(|row| {
                if self.wraps {
                    row.min_h(self.size.height()).py(room).items_start()
                } else {
                    row.h(self.size.height())
                }
            })
            .pl(room.size() + self.indent)
            .pr(room)
            .gap(room)
            .corners(Corner::Control)
            .typeset(Type::Body)
            .hand_while(clickable)
            .when(self.chosen, |row| row.bg(CHOSEN.of(colors.text)))
            .when(!self.chosen && on, |row| row.hover(|row| row.bg(UNDER_POINTER.of(colors.text))))
            .when(clickable, |row| row.active(|row| row.bg(PRESSED.of(colors.text))))
            .when_some(self.role, |row, (role, label)| {
                let row = row.role(role).aria_label(label);
                if role == Role::ListBoxOption { row.aria_selected(self.chosen) } else { row }
            })
            .when_some(self.tooltip, |row, words| row.says(words, cx))
            .when_some(self.on_click.filter(|_| on), |row, on_click| {
                row.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .when_some(self.on_hover, |row, on_hover| row.on_hover(on_hover))
            .when_some(self.on_press, |row, on_press| row.on_mouse_down(MouseButton::Left, on_press))
            .when_some(self.on_let_go, |row, on_let_go| row.on_mouse_up(MouseButton::Left, on_let_go))
            .when_some(self.on_let_go_outside, |row, on_let_go| row.on_mouse_up_out(MouseButton::Left, on_let_go))
            .children(self.children)
            .when(self.end == End::Chevron, |row| {
                let chevron = Icon::new(IconName::ChevronRight).sized(IconSize::Regular).text_color(colors.text_faint);
                row.child(div().flex_none().ml_auto().child(chevron))
            })
            .a11y_disabled(self.off);
        // On what holds the row, since the menu is a child of what it's on, set apart from the rest, and the row's
        // gap would count it as one of what's in the row.
        match self.menu {
            Some(build) => div()
                .flex_none()
                .w_full()
                .child(drawn)
                .right_click_menu(move |menu, window, cx| build(menu, window, cx))
                .into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
