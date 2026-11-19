use std::sync::Arc;

use gpui_kit::base::ScrollBounce;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div, px,
};
use look::{Hand as _, Shaped as _, Typeset as _};
use protocol::model::Proposal;

use super::answering::{Answering, Choosing, ProposalEvent};
use super::document::{self, Part, Writing};
use super::page::PageElement;
use crate::DictationHints;
use crate::linked_images::LinkedImages;
use crate::selection::Selection;
use crate::texts::{Of, Texts};
use client_core::Client;

/// What a proposal's line in the chat starts with, and its tab: a way that splits in two, for the options it offers.
pub const ICON: gpui_kit::assets::IconName = gpui_kit::assets::IconName::Split;
/// How wide the outline is, which is as wide as an option's label needs to be read at a glance.
const OUTLINE_WIDTH: f32 = 220.;
/// How wide the page's column is at most, as the chat's is, so its lines are as easy to read.
pub(super) const COLUMN: f32 = 848.;

/// One proposal's page, which the app shows in its tab.
pub struct ProposalView {
    pub(super) proposal: Arc<Proposal>,
    pub(super) answering: Answering,
    texts: Texts,
    /// The images what the agent wrote points to, which the hub kept with each revision, and the page is drawn again
    /// as each comes.
    linked: LinkedImages<()>,
    selection: Selection,
    /// The selection's, which a press on the page's text takes, so a copy and a select all come to it.
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// The tab's in front, so the note can be dictated into.
    pub(super) dictation_enabled: bool,
    pub(super) dictation_hints: Option<DictationHints>,
}

impl ProposalView {
    /// Shows `proposal`, reading the images it points to from the hub through `client`.
    pub fn new(proposal: Arc<Proposal>, client: Client, cx: &mut Context<Self>) -> Self {
        let this = cx.weak_entity();
        // A long text parsed on another thread is drawn once it's in.
        let texts = Texts::new(move |_, cx| {
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        let this = cx.weak_entity();
        let linked = LinkedImages::new(client, proposal.session_id.clone(), move |_, cx| {
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        let focus = cx.focus_handle();
        let selection = Selection::new(texts.clone(), focus.clone());
        let answering = Answering::default();
        Self {
            proposal,
            answering,
            texts,
            linked,
            selection,
            focus,
            scroll: ScrollHandle::new(),
            dictation_enabled: false,
            dictation_hints: None,
        }
    }

    /// Carries on on a new connection to the hub, and asks again for what couldn't be read.
    pub fn reconnect(&mut self, client: &Client) {
        self.linked.reconnect(client);
    }

    pub fn proposal(&self) -> &Arc<Proposal> {
        &self.proposal
    }

    /// Shows a newer copy of its record.
    pub fn set_proposal(&mut self, proposal: Arc<Proposal>, cx: &mut Context<Self>) {
        if !Arc::ptr_eq(&self.proposal, &proposal) {
            // What was chosen and written was for the revision read, so another starts afresh, rather than sending
            // the choices as an answer to options the person hasn't read.
            if proposal.current.number != self.proposal.current.number {
                self.end_dictation(cx);
                self.answering = Answering::default();
            }
            self.proposal = proposal;
            // Once it's answered, as from another client, there's no note to dictate into.
            self.follow_dictation(cx);
            cx.notify();
        }
    }

    /// The outline: each part, set in under its question when there are several, and the one at the top of the page
    /// marked. Pressing one scrolls the page to it.
    fn outline(&self, parts: &[Part], cx: &mut Context<Self>) -> AnyElement {
        let colors = look::colors(cx);
        let at_top = self.scroll.top_item();
        let rows = parts.iter().enumerate().map(|(ix, part)| {
            let here = ix == at_top;
            div()
                .id(("proposal-outline", ix))
                .w_full()
                .h(look::RowSize::Regular.height())
                .flex()
                .items_center()
                .px(look::Space::Small)
                .when(part.under, |row| row.pl(look::Space::XLarge))
                .corners(look::Corner::Control)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .typeset(look::Type::Body)
                .text_color(if here { colors.text } else { colors.text_muted })
                .when(here, |row| row.bg(colors.menu_hover))
                .hover(|row| row.bg(colors.menu_hover))
                .hand()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.scroll.scroll_to_top_of_item(ix);
                    cx.notify();
                }))
                .child(part.label.clone())
                .test_support()
        });
        v_flex()
            .id("proposal-outline")
            .flex_none()
            .w(px(OUTLINE_WIDTH))
            .h_full()
            .p(look::Space::Medium)
            .gap(look::Space::XSmall)
            .children(rows)
            .overflow_y_scrollbar()
            .into_any_element()
    }

    /// The page, which scrolls, with each part a child of what scrolls, so the outline finds it by its place.
    fn page(&self, parts: Vec<Part>, area: ElementId, cx: &App) -> AnyElement {
        let column = |part: Part| {
            div()
                .w_full()
                .px(look::Space::Large)
                .flex()
                .justify_center()
                .child(div().w_full().max_w(px(COLUMN)).child(part.element))
        };
        let scrolled = div()
            .id(area.clone())
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .pt(look::Space::Large)
            .text_color(look::colors(cx).text)
            .children(parts.into_iter().map(column))
            .test_support();
        let bounce = ScrollBounce::new("proposal-bounce", &self.scroll, scrolled).into_any_element();
        let page =
            PageElement { id: area, scroll: self.scroll.clone(), selection: self.selection.clone(), scrolled: bounce };
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(page)
            .vertical_scrollbar(&self.scroll)
            .into_any_element()
    }
}

impl gpui_kit::EventEmitter<ProposalEvent> for ProposalView {}

impl Focusable for ProposalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ProposalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let area = ElementId::Name("proposal-page".into());
        let parts = {
            let images = (&self.linked, self.proposal.current.images.as_slice());
            let mut writing =
                Writing { texts: &self.texts, selection: &self.selection, images, area: area.clone(), drawn: 0 };
            let choosing = Choosing {
                this: cx.weak_entity(),
                chosen: &self.answering.chosen,
                standing: self.proposal.current.standing(),
            };
            let revision = &self.proposal.current;
            // The page is drawn as one block, so its texts are the first of a page that isn't a chat.
            self.texts.drawing(Of::Document(0), || document::parts(revision, &mut writing, &choosing, cx))
        };
        let outline = self.outline(&parts, cx);
        let page = self.page(parts, area, cx);
        let foot = self.foot(window, cx);
        // Across the top of the page, as a banner is across a pane, while the agent writes the next revision.
        let banner = self.round_banner();
        let beside = v_flex().flex_1().min_w_0().h_full().children(banner).child(page).child(foot);
        self.selection.with_keys(div().flex().size_full().min_h_0().child(outline).child(beside))
    }
}
