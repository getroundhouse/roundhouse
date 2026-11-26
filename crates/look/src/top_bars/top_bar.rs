use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px, relative,
};

use crate::colors::colors;
use crate::controls::Button;
use crate::heights::BarSize;
use crate::icon_sizes::{IconSize, IconSizing as _};
use crate::lines::Lined as _;
use crate::menus::Menu;
use crate::region::Region;
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::theme::PRESSED;
use crate::typography::{Type, Typeset as _};

/// How far in from the bar's ends its buttons sit. A button there is a square as tall as the bar, so its icon is
/// further in than this, about where a phone's own apps draw theirs.
pub(super) const END: Space = Space::XSmall;
/// How tall a line of the title is, as a share of its text's size: close enough that the title and the line under it
/// fit in the bar, which a line of body text and one of detail wouldn't at their usual heights.
pub(super) const LINE: f32 = 1.2;

/// How the title's menu is built each time it opens.
type Build = Rc<dyn Fn(Menu, &mut Window, &mut App) -> Menu>;

/// A bar across the top of a handheld's screen, which names the screen `title`, with nothing else in it yet.
pub fn top_bar(title: impl Into<SharedString>) -> TopBar {
    TopBar {
        title: title.into(),
        under: None,
        mark: None,
        start: Vec::new(),
        end: Vec::new(),
        clear: px(0.),
        id: None,
        menu: None,
    }
}

/// A handheld's bar across the top of a screen, which [`top_bar`] makes. Its title is in its middle, however many
/// buttons are at either end, and is cut short only once it would run into them.
#[derive(IntoElement)]
#[must_use]
pub struct TopBar {
    title: SharedString,
    under: Option<SharedString>,
    mark: Option<AnyElement>,
    start: Vec<Button>,
    end: Vec<Button>,
    clear: Pixels,
    id: Option<ElementId>,
    menu: Option<(ElementId, Build)>,
}

impl TopBar {
    /// A line under its title, in a detail's text, saying more of what the screen shows, such as a workspace's
    /// branch.
    pub fn under(mut self, words: impl Into<SharedString>) -> Self {
        self.under = Some(words.into());
        self
    }

    /// What stands for what the title names, before it, such as a workspace's mark.
    pub fn mark(mut self, mark: impl IntoElement) -> Self {
        self.mark = Some(mark.into_any_element());
        self
    }

    /// A button at its start, such as the one that goes back.
    pub fn start(mut self, button: Button) -> Self {
        self.start.push(button);
        self
    }

    /// A button at its end, after any others there, for what's done on this screen.
    pub fn end(mut self, button: Button) -> Self {
        self.end.push(button);
        self
    }

    /// Taller by `top`, with what it holds under that, clear of what the device draws over the screen's top, such as
    /// its status bar, which the bar's surface runs up under.
    pub fn clear(mut self, top: Pixels) -> Self {
        self.clear = top;
        self
    }

    /// Has a press on its title open a menu under it, built by `build` each time it opens, and a chevron after the
    /// title say so, for a title that opens a choice of what the screen shows, such as a workspace's chats. Found by a
    /// test as `id`.
    pub fn menu(
        mut self,
        id: impl Into<ElementId>,
        build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static,
    ) -> Self {
        self.menu = Some((id.into(), Rc::new(build)));
        self
    }

    /// Found by a test as `region`, the part of the window it is.
    pub fn region(mut self, region: Region) -> Self {
        self.id = Some(region.id().into());
        self
    }

    /// Found by a test as `id`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl RenderOnce for TopBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let line = |kind: Type| div().typeset(kind).line_height(relative(LINE)).truncate();
        let names = v_flex()
            .min_w_0()
            .items_center()
            .child(line(Type::Body).emphasis().max_w_full().child(self.title.clone()))
            .when_some(self.under, |names, under| {
                names.child(line(Type::Detail).max_w_full().text_color(colors.text_muted).child(under))
            });
        let title = h_flex()
            .min_w_0()
            .flex_shrink(1.)
            .gap(Space::Small)
            .px(Space::Small)
            .children(self.mark.map(|mark| div().flex_none().child(mark)))
            .child(names);
        // The sides share what the title leaves evenly, so the title is in the bar's middle whatever is at its ends,
        // and gives way only once it would run into the wider of them. A side is never narrower than its buttons,
        // which a least width of its own would let it be.
        let side = || h_flex().flex_1().flex_shrink_0().h_full();
        let title = match self.menu {
            Some((id, build)) => {
                let drawn = title
                    .child(Icon::new(IconName::ChevronDown).sized(IconSize::Small).text_color(colors.text_muted))
                    .id(id)
                    .test_support()
                    .role(Role::Button)
                    .aria_label(self.title)
                    .py(Space::XSmall)
                    .corners(Corner::Control);
                crate::menus::under_the_title(drawn, PRESSED.of(colors.text), move |menu, window, cx| {
                    build(menu, window, cx)
                })
                .into_any_element()
            }
            None => title.into_any_element(),
        };
        let row = h_flex()
            .w_full()
            .h(BarSize::Regular.height())
            .px(END)
            .child(side().children(self.start))
            .child(title)
            .child(side().justify_end().children(self.end));
        let drawn =
            div().flex_none().w_full().pt(self.clear).bg(colors.pane).text_color(colors.text).line_under(cx).child(row);
        match self.id {
            Some(id) => drawn.id(id).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
