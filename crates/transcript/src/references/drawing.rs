//! A reference among a reply's words, as the design system's small chip, with its card while the pointer rests on it,
//! and its menu on a right-click, the same on every chip.

use gpui_kit::component::text::MarkdownParseContext;
use gpui_kit::component::text::markdown_ast::Node;
use gpui_kit::component::text::{InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode, MarkdownPlugin};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, TestSupportExt as _,
    Window, div,
};
use look::{ChipKind, GithubState, IssueState, PullRequestState, RightClickMenu as _, Typeset as _};
use protocol::model::{Reference, ReferenceKind, ReferenceRead};

use super::card::{card, menu};
use super::kept::References;
use super::reading::{Named, REFERENCE};
use crate::linked_images::Waiter;

/// A chip among a paragraph's words is a small one, shorter than their line.
const SIZE: look::ChipSize = look::ChipSize::Small;

/// What the drawing of a text by `drawer` takes, as well as `extensions`, to draw what it names from `references`.
pub(crate) fn drawing<W: Waiter>(
    extensions: MarkdownExtensions,
    references: &References<W>,
    drawer: W,
) -> MarkdownExtensions {
    extensions.plugin(DrawsReferences { references: references.clone(), drawer })
}

struct DrawsReferences<W> {
    references: References<W>,
    drawer: W,
}

impl<W: Waiter> MarkdownPlugin for DrawsReferences<W> {
    fn name(&self) -> &str {
        REFERENCE
    }

    fn parse(&self, _: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        None
    }

    /// The chip, once the hub has found what it names, and otherwise the link as it was written.
    fn render_inline(
        &self,
        node: &MarkdownNode,
        _: &InlineRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let named = node.data::<Named>()?.clone();
        let Some(ReferenceRead::Found { reference }) = self.references.read(&named.ask, self.drawer, cx) else {
            return None;
        };
        let at = node.source_range().map_or(0, |at| at.start);
        let chip = chip(&named, *reference, at, self.references.clone())?;
        // It's drawn as the link, among the words around it, whose underline, and any bold, italics or line through
        // them, the chip, its card and its menu would take.
        let chip = div().apart_from_its_text().child(chip);
        Some(InlineElement::new(chip).with_baseline(SIZE.baseline(window, cx)))
    }
}

/// The design system's chip for `reference`, as `named` names it at byte `at` of its text, with its card while the
/// pointer rests on it, and its menu. `None` for what this version can't draw.
fn chip<W: Waiter>(
    named: &Named,
    reference: Reference,
    at: usize,
    references: References<W>,
) -> Option<gpui_kit::AnyElement> {
    let (kind, state) = kind_and_state(&reference)?;
    let label = references.label(&reference);
    let chip = look::chip(kind, SIZE, label.clone())
        .state(state)
        .when(named.first, |chip| chip.title(reference.title.clone()));
    // The first mention, which is the one a text titles, is known by its label alone, and a later one by where it
    // is, too, so none share an element.
    let id = |what: &str| {
        let name = if named.first { format!("{what}/{label}") } else { format!("{what}/{label}/{at}") };
        ElementId::Name(name.into())
    };
    let (for_menu, for_card, menu_references) = (reference.clone(), reference, references.clone());
    // A click opens the link it's the words of, as the kit opens any link, to where the link goes, such as a comment.
    let chip = div().id(id("reference")).flex_none().child(chip).test_support().right_click_menu(move |list, _, cx| {
        let tie = menu_references.tie(&for_menu);
        menu(&for_menu, &tie, &menu_references, list, cx)
    });
    Some(
        look::hover_card_with_a_button(id("reference-card"))
            .anchor(Anchor::TopLeft)
            .trigger(chip)
            .content(move |_, _, cx| {
                let tie = references.tie(&for_card);
                card(&for_card, state, &tie, &references, cx)
            })
            .into_any_element(),
    )
}

/// The kind of chip `reference` is drawn as, and its state, or `None` for a kind or a state this version doesn't
/// know, which is drawn as it was written.
pub(super) fn kind_and_state(reference: &Reference) -> Option<(ChipKind, GithubState)> {
    use protocol::model::{IssueState as Issue, PullRequestState as Pull};
    match reference.kind {
        ReferenceKind::Issue { state } => {
            let state = match state {
                Issue::Open => IssueState::Open,
                Issue::Done => IssueState::Done,
                Issue::NotPlanned => IssueState::NotPlanned,
                Issue::Unknown => return None,
            };
            Some((ChipKind::Issue, state.into()))
        }
        ReferenceKind::PullRequest(pull_request) => {
            let state = match pull_request.state {
                Pull::Open => PullRequestState::Open,
                Pull::Draft => PullRequestState::Draft,
                Pull::Merged => PullRequestState::Merged,
                Pull::Closed => PullRequestState::Closed,
                Pull::Unknown => return None,
            };
            Some((ChipKind::PullRequest, state.into()))
        }
        ReferenceKind::Unknown => None,
    }
}
