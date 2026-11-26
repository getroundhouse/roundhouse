use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels, RenderOnce, Styled as _,
    TestSupportExt as _, Window, px,
};

use super::BannerKind;
use crate::bars::BarKind;
use crate::cards::WASH;
use crate::colors::colors;
use crate::control_sizes::ControlSize;
use crate::controls::{Button, MenuButton, Side};
use crate::heights::BarSize;
use crate::icon_sizes::{IconSize, IconSizing as _};
use crate::region::Region;
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// How big a banner's icon is, beside its words.
pub(super) const ICON: IconSize = IconSize::Regular;

/// A banner of `kind`, with nothing in it yet.
pub fn banner(kind: BannerKind) -> Banner {
    Banner { kind, clear: (px(0.), px(0.)), id: None, words: Vec::new(), answers: Vec::new() }
}

/// A strip across a pane, or the window's top, that says something about all of it, which [`banner`] makes: washed
/// in its kind's colour, with its kind's icon, what it says, and the buttons that do something about it at its end.
/// What's in it starts and ends as far in as what's in a bar does, and it's at least as tall as a small bar. Words that
/// wrap make it taller, with its icon and buttons at their first line.
#[derive(IntoElement)]
#[must_use]
pub struct Banner {
    kind: BannerKind,
    clear: (Pixels, Pixels),
    id: Option<ElementId>,
    words: Vec<AnyElement>,
    answers: Vec<Answer>,
}

/// A button at a banner's end: one that does something, or one that opens a menu of what to do.
enum Answer {
    Press(Button),
    Menu(MenuButton),
}

impl Answer {
    /// Out past its own room at its end.
    fn stood_out(self) -> Self {
        match self {
            Answer::Press(button) => Answer::Press(button.stands_out(&[Side::Right])),
            Answer::Menu(button) => Answer::Menu(button.stands_out(&[Side::Right])),
        }
    }

    fn drawn(self) -> AnyElement {
        match self {
            Answer::Press(button) => button.into_any_element(),
            Answer::Menu(button) => button.into_any_element(),
        }
    }
}

impl Banner {
    /// A button that does something about what it says, at its end, after the ones before it. The last stands out
    /// past its own room, so its words end as far in as what the banner says starts.
    pub fn answer(mut self, button: Button) -> Self {
        self.answers.push(Answer::Press(button));
        self
    }

    /// A button that opens a menu of what to do about what it says, at its end, after the ones before it, standing out
    /// as [`Self::answer`]'s does.
    pub fn answer_menu(mut self, button: MenuButton) -> Self {
        self.answers.push(Answer::Menu(button));
        self
    }

    /// Further in by `start` and `end`, clear of what's drawn over its ends, such as the window's buttons.
    pub fn clear(mut self, start: Pixels, end: Pixels) -> Self {
        self.clear = (start, end);
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

/// `answers`, the last out past its own room, so its words end as far in as what the banner says starts.
fn stood_out(mut answers: Vec<Answer>) -> impl Iterator<Item = AnyElement> {
    if let Some(last) = answers.pop() {
        answers.push(last.stood_out());
    }
    answers.into_iter().map(Answer::drawn)
}

impl ParentElement for Banner {
    /// What it says, after its icon.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.words.extend(elements);
    }
}

impl RenderOnce for Banner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let tone = self.kind.tone(colors);
        let (start, end) = self.clear;
        // What it says can wrap, so its icon and its buttons sit at its first line, each part at least a small
        // button's height, which a line is centred in.
        let line = ControlSize::Small.height();
        let drawn = h_flex()
            .items_start()
            .flex_none()
            .w_full()
            .min_h(BarSize::Small.height())
            .pl(BarKind::Words.start().size() + start)
            .pr(BarKind::END.size() + end)
            .py(Space::XSmall)
            .gap(Space::Small)
            .bg(WASH.of(tone))
            .typeset(Type::Body)
            .text_color(colors.text)
            .child(h_flex().flex_none().h(line).child(Icon::new(self.kind.icon()).sized(ICON).text_color(tone)))
            .child(v_flex().flex_1().min_w_0().min_h(line).justify_center().children(self.words))
            .children(stood_out(self.answers));
        match self.id {
            Some(id) => drawn.id(id).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
