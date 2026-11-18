use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled as _, TestSupportExt as _, Window, div,
};

use super::{CardKind, CardWidth};
use crate::colors::colors;
use crate::icon_sizes::{IconSize, IconSizing as _};
use crate::raised::shadow;
use crate::region::Region;
use crate::shape::{Corner, EDGE, Shaped as _};
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// A card of `kind`, as wide as `width`, with nothing in it yet.
pub fn card(kind: CardKind, width: CardWidth) -> Card {
    Card { kind, width, roomy: false, heading: None, under: Vec::new(), parts: Vec::new(), id: None }
}

/// A card's heading, which says `words`, in medium weight. [`Card::heading`] puts it at the card's top.
pub fn card_heading(words: impl Into<SharedString>) -> CardHeading {
    CardHeading { words: words.into(), mark: None, after: Vec::new(), end: None }
}

/// One of a card's parts.
enum Part {
    /// What's put in it.
    Element(AnyElement),
    /// A line across it, between the parts on either side.
    Rule,
}

/// What a card is known by, to a test and to an assistive app.
enum Known {
    /// A part of the window a test finds by where it's drawn.
    Region(Region),
    /// An ID of its own.
    Id(ElementId),
}

/// A few parts about one thing, apart from what's around it, which [`card`] makes. What's put in it are its parts,
/// under its heading, each set apart from the next.
#[derive(IntoElement)]
#[must_use]
pub struct Card {
    kind: CardKind,
    width: CardWidth,
    roomy: bool,
    heading: Option<CardHeading>,
    under: Vec<AnyElement>,
    parts: Vec<Part>,
    id: Option<Known>,
}

impl Card {
    /// Its heading, at its top.
    pub fn heading(mut self, heading: CardHeading) -> Self {
        self.heading = Some(heading);
        self
    }

    /// Something said under its heading, as part of it, such as the day a turn ended or a hover card's lines, in the
    /// order they're added.
    pub fn under_heading(mut self, element: impl IntoElement) -> Self {
        self.under.push(element.into_any_element());
        self
    }

    /// A line across it, between the part before and the next, such as between a path and what's said of it. It's
    /// as far from each as the parts are from each other.
    pub fn rule(mut self) -> Self {
        self.parts.push(Part::Rule);
        self
    }

    /// The part of the window it is, which a test finds it by.
    pub fn region(mut self, region: Region) -> Self {
        self.id = Some(Known::Region(region));
        self
    }

    /// Its ID, which a test finds it by.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(Known::Id(id.into()));
        self
    }

    /// Laid out as a dialog is, with more room around and between its parts, for a form set in place, such as a panel
    /// in Settings.
    pub fn roomy(mut self) -> Self {
        self.roomy = true;
        self
    }
}

impl ParentElement for Card {
    /// One of its parts, under its heading and the parts before it.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.parts.extend(elements.into_iter().map(Part::Element));
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let kind = self.kind;
        let (around, between) = if self.roomy { (Space::XLarge, Space::Large) } else { (Space::Medium, Space::Medium) };
        let heading = self.heading.map(|heading| heading.drawn(kind, cx));
        let top = (heading.is_some() || !self.under.is_empty())
            .then(|| v_flex().gap(Space::XSmall).children(heading).children(self.under).into_any_element());
        let drawn = v_flex()
            .flex_none()
            .when_some(self.width.width(), gpui_kit::Styled::w)
            .when(self.width == CardWidth::Fills, gpui_kit::Styled::w_full)
            .p(around)
            .gap(between)
            .corners(Corner::Card)
            .bg(kind.surface(colors))
            .when(kind.edged(), |card| card.border(EDGE).border_color(colors.edge))
            .when(kind.casts_a_shadow(), |card| card.shadow(shadow(colors)))
            .typeset(Type::Body)
            .text_color(colors.text)
            .children(top)
            .children(self.parts.into_iter().map(|part| match part {
                Part::Element(element) => element,
                Part::Rule => div().flex_none().h(EDGE).bg(colors.edge).into_any_element(),
            }));
        match self.id {
            Some(Known::Region(region)) => drawn.id(region.id()).test_support().into_any_element(),
            Some(Known::Id(id)) => drawn.id(id).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}

/// A card's heading, which [`card_heading`] makes: its words, the icon at their start that stands for what the card
/// is, what's beside them, and what's said at the line's end.
#[must_use]
pub struct CardHeading {
    words: SharedString,
    mark: Option<IconName>,
    after: Vec<AnyElement>,
    end: Option<SharedString>,
}

impl CardHeading {
    /// The icon at its start, which stands for what the card is, such as a failed request's.
    pub fn mark(mut self, icon: IconName) -> Self {
        self.mark = Some(icon);
        self
    }

    /// Something after its words, such as a badge of how many times.
    pub fn after(mut self, element: impl IntoElement) -> Self {
        self.after.push(element.into_any_element());
        self
    }

    /// What's said at the line's end, faint, such as when a limit resets.
    pub fn end(mut self, words: impl Into<SharedString>) -> Self {
        self.end = Some(words.into());
        self
    }

    /// Drawn in a card of `kind`: in its colour when it has one, and otherwise in the text's, with its mark muted.
    fn drawn(self, kind: CardKind, cx: &App) -> AnyElement {
        let colors = colors(cx);
        let tint = kind.tint(colors);
        let mark = self.mark.map(|mark| {
            Icon::new(mark).flex_none().sized(IconSize::Large).text_color(tint.unwrap_or(colors.text_muted))
        });
        let end = self
            .end
            .map(|end| div().ml_auto().flex_none().typeset(Type::Detail).text_color(colors.text_faint).child(end));
        h_flex()
            .gap(Space::Small)
            .children(mark)
            .child(div().min_w_0().emphasis().text_color(tint.unwrap_or(colors.text)).child(self.words))
            .children(self.after)
            .children(end)
            .into_any_element()
    }
}
