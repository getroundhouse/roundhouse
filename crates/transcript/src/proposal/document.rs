//! The page's parts, from what the agent wrote, in the order they're read: its context, then each question, its
//! question and what compares its options, each option as a card, and the option the agent recommends, and why. Each
//! part is one the outline names and scrolls to.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, ElementId, IntoElement, ParentElement as _, SharedString, Styled as _, div};
use look::Typeset as _;
use protocol::model::{OptionLetter, OptionPlace, ProposalOption, ProposalQuestion, ProposalRevision, RevisionNumber};

use super::answering::Choosing;
use crate::find::{Marks, marked_with_layout};
use crate::linked_images::LinkedImages;
use crate::selection::{Pieces, Selection};
use crate::texts::{Plain, TextAt, Texts, Written};
use protocol::event::LinkedImage;

/// One part of the page.
pub(super) struct Part {
    /// What the outline calls it.
    pub(super) label: SharedString,
    /// Whether the outline sets it in, under its question, which it does only when there's more than one question.
    pub(super) under: bool,
    pub(super) element: AnyElement,
}

/// What draws the page's texts, each in its place, in the order they're drawn, and selected with the rest.
pub(super) struct Writing<'a> {
    pub(super) texts: &'a Texts,
    pub(super) selection: &'a Selection,
    /// Where the images it points to come from, and the copies the hub kept of them.
    pub(super) images: (&'a LinkedImages<()>, &'a [LinkedImage]),
    /// The page's area, which scrolls every text on it.
    pub(super) area: ElementId,
    /// How many texts it's drawn so far.
    pub(super) drawn: usize,
}

impl Writing<'_> {
    fn next(&mut self) -> TextAt {
        self.drawn += 1;
        TextAt::on_page(self.drawn - 1)
    }

    /// What the agent wrote in Markdown, known by `key`, which stays the same while it's the same text.
    fn markdown(&mut self, key: &str, text: &str) -> AnyElement {
        let written = Written::unfound(self.next(), ElementId::Name(key.to_string().into()), text);
        let (linked, kept) = self.images;
        self.texts.draw(self.selection, &written.in_area(self.area.clone()), Some(linked.images((), kept)))
    }

    /// What the agent wrote as plain text, such as a title, which is selected and copied as its Markdown is.
    fn plain(&mut self, key: &str, text: &str) -> AnyElement {
        let said: SharedString = text.trim().to_string().into();
        let (drawn, layout) = marked_with_layout(said.clone(), &Marks::default(), &[], Some(self.area.clone()));
        let element = ElementId::Name(key.to_string().into());
        let plain = Plain {
            at: self.next().into(),
            element,
            area: self.area.clone(),
            drawn,
            said,
            pieces: Pieces::whole(layout),
        };
        self.texts.draw_plain(self.selection, plain)
    }
}

/// Every part of the page `revision` makes: what changed in it, when it's one the agent revised, then what it proposes.
pub(super) fn parts(
    revision: &ProposalRevision,
    writing: &mut Writing<'_>,
    choosing: &Choosing<'_>,
    cx: &App,
) -> Vec<Part> {
    let mut parts = Vec::new();
    if !revision.changed.trim().is_empty() {
        let before = RevisionNumber(revision.number.0.saturating_sub(1));
        let changed = writing.markdown("changed", &revision.changed);
        let heading = div().typeset(look::Type::Heading).child(format!("What changed since revision {before}"));
        let element = section(heading).child(changed).into_any_element();
        parts.push(Part { label: "What changed".into(), under: false, element });
    }
    let proposed = &revision.proposed;
    if !proposed.context.trim().is_empty() {
        let context = writing.markdown("context", &proposed.context);
        let element = section(heading("Context")).child(context).into_any_element();
        parts.push(Part { label: "Context".into(), under: false, element });
    }
    let several = proposed.questions.len() > 1;
    for (at, question) in proposed.questions.iter().enumerate() {
        parts.push(asked(at, question, writing, cx));
        for place in question.places() {
            let Some((letter, option)) = question.named(place) else { continue };
            let recommended = question.recommended.as_ref().is_some_and(|r| r.option == place);
            let label = format!("{letter} · {}", option.label.trim()).into();
            let choose = choosing.button(at, place, letter);
            let element = option_card(at, (place, letter), option, recommended, choose, writing, cx);
            parts.push(Part { label, under: several, element });
        }
        if let Some(part) = recommendation(at, question, writing) {
            parts.push(Part { under: several, ..part });
        }
    }
    parts
}

/// What a question asks: its header, its text, and what compares its options, such as a table.
fn asked(at: usize, question: &ProposalQuestion, writing: &mut Writing<'_>, cx: &App) -> Part {
    let colors = look::colors(cx);
    let header =
        div().typeset(look::Type::Detail).text_color(colors.text_muted).child(question.header.trim().to_string());
    let text = div().typeset(look::Type::Heading).child(writing.plain(&format!("question-{at}"), &question.text));
    let details =
        (!question.details.trim().is_empty()).then(|| writing.markdown(&format!("details-{at}"), &question.details));
    let element = section(v_flex().gap(look::Space::XSmall).child(header).child(text)).children(details);
    Part { label: question.header.trim().to_string().into(), under: false, element: element.into_any_element() }
}

/// An option, as a card: its letter, its label, whether it's recommended, Choose at its end, its description and its
/// body.
fn option_card(
    at: usize,
    (place, letter): (OptionPlace, OptionLetter),
    option: &ProposalOption,
    recommended: bool,
    choose: Option<AnyElement>,
    writing: &mut Writing<'_>,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let key = |part: &str| format!("option-{at}-{}-{part}", place.index());
    let label = div().min_w_0().typeset(look::Type::Heading).child(writing.plain(&key("label"), &option.label));
    let named = h_flex()
        .flex_1()
        .min_w_0()
        .gap(look::Space::Small)
        .child(look::badge(look::BadgeKind::Plain, letter.to_string()))
        .child(label)
        .when(recommended, |named| named.child(look::badge(look::BadgeKind::Plain, "Recommended")));
    let top = h_flex()
        .gap(look::Space::Small)
        .child(named)
        .when_some(choose, |top, choose| top.child(div().flex_none().child(choose)));
    let description = (!option.description.trim().is_empty()).then(|| {
        div()
            .typeset(look::Type::Body)
            .text_color(colors.text_muted)
            .child(writing.plain(&key("description"), &option.description))
    });
    let body = (!option.body.trim().is_empty()).then(|| writing.markdown(&key("body"), &option.body));
    // In the proposal's tab rather than the chat, so set in place.
    let card =
        look::card(look::CardKind::InPlace, look::CardWidth::Fills).child(top).children(description).children(body);
    spaced(card).into_any_element()
}

/// The option the agent recommends, by its letter and label, and why, when it recommends one.
fn recommendation(at: usize, question: &ProposalQuestion, writing: &mut Writing<'_>) -> Option<Part> {
    let recommended = question.recommended.as_ref()?;
    let (letter, option) = question.named(recommended.option)?;
    let named = format!("{letter} · {}", option.label.trim());
    let named = div().typeset(look::Type::Body).child(writing.plain(&format!("recommended-{at}"), &named));
    let why = (!recommended.why.trim().is_empty()).then(|| writing.markdown(&format!("why-{at}"), &recommended.why));
    let element = section(heading("Recommendation")).child(named).children(why).into_any_element();
    Some(Part { label: "Recommendation".into(), under: false, element })
}

/// A part with its heading, and the space below it that sets it apart from the next.
fn section(heading: impl IntoElement) -> Div {
    spaced(v_flex().w_full().gap(look::Space::Small).child(heading))
}

fn spaced(part: impl IntoElement) -> Div {
    div().w_full().pb(look::Space::XLarge).child(part)
}

fn heading(text: &'static str) -> Div {
    div().typeset(look::Type::Heading).child(text)
}
