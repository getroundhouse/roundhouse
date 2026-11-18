use std::rc::Rc;

use gpui_kit::base::Tab as Pressable;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div, relative,
};

use super::TabWidth;
use crate::baseline::LineText;
use crate::colors::colors;
use crate::controls::Button;
use crate::counts::{CountKind, count};
use crate::hand::Hand as _;
use crate::menus::{Menu, RightClickMenu as _};
use crate::shape::TAB_LINE;
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// How far in from a tab's start its mark or its name starts.
pub(crate) const TAB_START: Space = Space::Medium;
/// Each tab's, so what shows while the pointer is over a tab shows only on that tab.
const GROUP: &str = "look-tab";
/// How strong the line under a tab is while the pointer is over it, of the line under the tab showing, as the owner
/// chose from pictures: there, but quieter than the tab showing.
const UNDER_POINTER: f32 = 0.35;

/// A tab of `width` that says `name`, found by `id`, which shows what it names when it's clicked.
pub fn tab(id: impl Into<SharedString>, name: impl Into<SharedString>, width: TabWidth) -> Tab {
    Tab {
        id: id.into(),
        name: name.into(),
        width,
        label: None,
        mark: None,
        count: None,
        after: Vec::new(),
        replaced: false,
        needs_a_look: false,
        showing: false,
        closes: None,
        tooltip: None,
        on_click: None,
        menu: None,
    }
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type Build = Rc<dyn Fn(Menu, &mut Window, &mut App) -> Menu>;

/// One of what a bar of tabs can show, which [`tab`] makes: its mark, its name and what's said after it, underlined
/// while it's showing.
#[derive(IntoElement)]
#[must_use]
pub struct Tab {
    id: SharedString,
    name: SharedString,
    width: TabWidth,
    label: Option<SharedString>,
    mark: Option<AnyElement>,
    count: Option<usize>,
    after: Vec<AnyElement>,
    replaced: bool,
    needs_a_look: bool,
    showing: bool,
    /// Its close button, as an element, since a whole button would make a tab too big to build a row of.
    closes: Option<AnyElement>,
    tooltip: Option<SharedString>,
    on_click: Option<OnClick>,
    menu: Option<Build>,
}

impl Tab {
    /// What stands for what it names, before its name, such as a chat's mark or a terminal's icon.
    pub fn mark(mut self, mark: impl IntoElement) -> Self {
        self.mark = Some(mark.into_any_element());
        self
    }

    /// How many of what it shows there are, after its name, such as the files Changes lists.
    pub fn count(mut self, how_many: usize) -> Self {
        self.count = Some(how_many);
        self
    }

    /// What's said after its name, such as a chat's cache about to expire.
    pub fn after(mut self, element: impl IntoElement) -> Self {
        self.after.push(element.into_any_element());
        self
    }

    /// What it's called to assistive apps, where that says more than its name, such as a file tab's whole path.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Its name in italics, for a tab that's replaced rather than added to, such as the file tab.
    pub fn replaced(mut self) -> Self {
        self.replaced = true;
        self
    }

    /// Its name stronger and in the text's colour, for what needs a look, such as a reply not yet read.
    pub fn needs_a_look(mut self, needs_a_look: bool) -> Self {
        self.needs_a_look = needs_a_look;
        self
    }

    /// Underlined, as what the bar shows.
    pub fn showing(mut self, showing: bool) -> Self {
        self.showing = showing;
        self
    }

    /// `button` at its end, which closes it, shown while the pointer is over it.
    pub fn closes(mut self, button: Button) -> Self {
        self.closes = Some(button.into_any_element());
        self
    }

    /// What it names, while the pointer rests on it, where its name is cut short.
    pub fn tooltip(mut self, words: impl Into<SharedString>) -> Self {
        self.tooltip = Some(words.into());
        self
    }

    /// What a click on it does: shows what it names.
    pub fn on_click(mut self, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(on_click));
        self
    }

    /// The menu `build` fills, where it's right-clicked.
    pub fn right_click_menu(mut self, build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static) -> Self {
        self.menu = Some(Rc::new(build));
        self
    }
}

impl RenderOnce for Tab {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let even = self.width == TabWidth::Even;
        let name = div()
            .when(even, |name| name.flex_1().min_w_0().overflow_hidden().text_ellipsis())
            .when(!even, Styled::flex_none)
            .whitespace_nowrap()
            .when(self.replaced, Styled::italic)
            .child(self.name.clone());
        // A count is smaller than the name, so both go on the name's baseline.
        let text = self.count.map(|how_many| (how_many, LineText::new(window, cx)));
        let (name, count) = match &text {
            Some((how_many, text)) => {
                (text.body.set(name), Some(count(CountKind::HowMany, *how_many).placed(&text.detail)))
            }
            None => (name, None),
        };
        let pressable = Pressable::new(ElementId::Name(self.id.clone()))
            .hand()
            .selected(self.showing)
            .accessibility_label(self.label.unwrap_or(self.name))
            .h_full()
            .when(even, |tab| tab.w_full().justify_start())
            .gap(Space::Small)
            .pl(TAB_START)
            .pr(if self.closes.is_some() { Space::Small } else { Space::Medium })
            .when(text.is_none(), |tab| tab.typeset(Type::Body).line_height(relative(1.25)))
            .map(|tab| {
                if self.needs_a_look {
                    tab.text_color(colors.text).emphasis()
                } else {
                    tab.text_color(colors.text_muted)
                }
            })
            .when_some(self.tooltip, |tab, words| tab.tooltip(crate::tooltip(words)))
            .when_some(self.on_click, |tab, on_click| {
                tab.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .children(self.mark)
            .child(name)
            .children(count)
            .children(self.after)
            // Hidden rather than left out, so the name doesn't move when it shows.
            .when_some(self.closes, |tab, close| {
                tab.child(div().flex_none().invisible().group_hover(GROUP, Styled::visible).child(close))
            });
        let drawn = div()
            .id(ElementId::Name(format!("{}-tab", self.id).into()))
            .test_support()
            .group(GROUP)
            .relative()
            .flex_none()
            .h_full()
            .when_some(self.width.width(), Styled::w)
            .child(pressable)
            .child(line(self.showing, cx));
        match self.menu {
            Some(build) => drawn.right_click_menu(move |menu, window, cx| build(menu, window, cx)).into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}

/// The line under a tab, over the bar's bottom edge: the tab showing's, or a quieter one while the pointer's over
/// another.
fn line(showing: bool, cx: &App) -> Div {
    let colors = colors(cx);
    let line = div().absolute().left_0().right_0().bottom_0().h(TAB_LINE);
    if showing {
        line.bg(colors.current)
    } else {
        line.bg(colors.current.opacity(UNDER_POINTER)).invisible().group_hover(GROUP, Styled::visible)
    }
}
