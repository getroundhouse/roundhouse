use std::sync::Arc;

use gpui_kit::base::ScrollBounce;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div, point, px,
};
use protocol::model::Proposal;

use super::answering::{Answering, Choosing, ProposalEvent};
use super::document::{self, Part, Writing};
use crate::DictationHints;
use crate::linked_images::LinkedImages;
use crate::references::References;
use crate::selection::{Scrolled, Selection, Way};
use crate::step_keys::{Steps, with_step_keys};
use crate::stops::{Within, stop_beside};
use crate::texts::{Of, TextAt, Texts};
use client_core::records::Snapshot;
use client_core::{Hub, RecordsFeed};

/// What a proposal's line in the chat starts with, and its tab: a way that splits in two, for the options it offers.
pub const ICON: gpui_kit::assets::IconName = gpui_kit::assets::IconName::Split;
/// How wide the outline is, which is as wide as an option's label needs to be read at a glance.
const OUTLINE_WIDTH: f32 = 220.;

/// How much further in a part under its question is set than the question, so it reads as the question's.
const UNDER: look::Space = look::Space::Medium;
/// How wide the page's column is at most, as the chat's is, so its lines are as easy to read.
pub(super) const COLUMN: f32 = 848.;

/// One proposal's page, which the app shows in its tab.
pub struct ProposalView {
    /// The record whose newest revision the page shows.
    pub(super) proposal: Arc<Proposal>,
    /// The record as the hub has it now, while the page stays on a revision the agent has since replaced, as `held`
    /// says.
    pub(super) newest: Option<Arc<Proposal>>,
    pub(super) answering: Answering,
    texts: Texts,
    /// The images what the agent wrote points to, which the hub kept with each revision, and the page is drawn again
    /// as each comes.
    linked: LinkedImages<()>,
    /// The issues and pull requests what the agent wrote links to, which a chip names by its number alone in the
    /// proposal's workspace's repository.
    references: References<()>,
    /// What answers are sent to the hub through.
    pub(super) hub: Hub,
    pub(super) selection: Selection,
    /// The selection's, which a press on the page's text takes, so a copy and a select all come to it.
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// Where each part's first text is among the page's, as it was last drawn, for the keys that move the end of
    /// what's selected to a part's start.
    starts: Vec<usize>,
    /// The tab's in front, in a workspace that's showing, so the person may be reading it: the note can be dictated
    /// into, and a revision the agent writes doesn't take the page.
    pub(super) in_sight: bool,
    pub(super) dictation_hints: Option<DictationHints>,
}

impl ProposalView {
    /// Shows `proposal`, reading the images it points to from `hub`, and sending its answer there.
    pub fn new(proposal: Arc<Proposal>, hub: Hub, cx: &mut Context<Self>) -> Self {
        let this = cx.weak_entity();
        // A long text parsed on another thread is drawn once it's in.
        let texts = Texts::new(move |_, cx| {
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        let this = cx.weak_entity();
        let linked = LinkedImages::new(hub.clone(), proposal.session_id.clone(), move |_, cx| {
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        let (changed, go_to) = (cx.weak_entity(), cx.weak_entity());
        let references = References::new(
            proposal.workspace_id.clone(),
            move |_, cx| {
                let _ = changed.update(cx, |_, cx| cx.notify());
            },
            move |workspace, cx| {
                let _ = go_to.update(cx, |_, cx| cx.emit(ProposalEvent::GoToWorkspace(workspace)));
            },
            // Each chip it draws is in what it shows.
            |(), _| true,
        );
        let focus = cx.focus_handle();
        let selection = Selection::new(texts.clone(), focus.clone());
        let answering = Answering::default();
        Self {
            proposal,
            newest: None,
            answering,
            texts,
            linked,
            references,
            hub,
            selection,
            starts: Vec::new(),
            focus,
            scroll: ScrollHandle::new(),
            in_sight: false,
            dictation_hints: None,
        }
    }

    /// Reads which workspace a pull request the page names is in, and what the hub says of it, from `records` from now
    /// on.
    pub fn follow_records(&self, records: RecordsFeed) {
        self.references.follow_records(records);
    }

    /// Takes what the client holds now of what the hub says of the issues and pull requests the page's chips name,
    /// drawing again those it changed, and asks again for the images it couldn't read, once a new connection has caught
    /// up. A client calls this whenever its records change, which they do once a new connection has caught up.
    pub fn records_changed(&self, records: &Snapshot, cx: &mut App) {
        self.linked.caught_up_again(cx);
        self.references.records_changed(records, cx);
    }

    /// The record whose newest revision the page shows, which isn't the hub's newest while that's held back.
    pub fn proposal(&self) -> &Arc<Proposal> {
        &self.proposal
    }

    /// Scrolls the page back to its top, where another revision starts with what changed.
    pub(super) fn scroll_to_top(&self) {
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    /// Scrolls the page to the top of its `part`, as pressing it in the outline and a step do. What was being brought
    /// into view gives way, as it does to the keys that scroll a page, rather than scroll back to it after.
    fn go_to_part(&self, part: usize, cx: &mut Context<Self>) {
        self.selection.reveal().done();
        self.scroll.scroll_to_top_of_item(part);
        cx.notify();
    }

    /// The outline: each part, set in under its question when there are several, and the one at the top of the page
    /// marked. Pressing one scrolls the page to it.
    fn outline(&self, parts: &[Part], cx: &mut Context<Self>) -> AnyElement {
        let colors = look::colors(cx);
        let at_top = self.scroll.top_item();
        let rows = parts.iter().enumerate().map(|(ix, part)| {
            let here = ix == at_top;
            let label = div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(if here { colors.text } else { colors.text_muted })
                .child(part.label.clone());
            look::list_row(("proposal-outline", ix), look::RowSize::Regular)
                .chosen(here)
                .when(part.under, |row| row.indent(UNDER.size()))
                .on_click(cx.listener(move |this, _, _, cx| this.go_to_part(ix, cx)))
                .child(label)
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
        let page = Scrolled::new(area, self.scroll.clone(), &self.selection, bounce);
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

impl Steps for ProposalView {
    /// Steps the page `way` to the part of its outline before the top or after it, with that at the top, as pressing
    /// that part in the outline does. Each part is a child of what scrolls.
    fn step(&mut self, way: Way, cx: &mut Context<Self>) {
        // Where the top part's top is, from the page's, which is above it once the page is scrolled into it.
        let (top, starts) = self.scroll.logical_scroll_top();
        if let Some(part) = stop_beside(0..self.scroll.children_count(), top, Within::scrolled(-starts), way) {
            self.go_to_part(part, cx);
        }
    }

    /// Moves the end of what's selected `way` to the start of a part of the outline.
    fn select_step(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        // Every text on the page is placed by where it's drawn among them, so a part's start is its first text's.
        let Some((end, within)) = self.selection.far_end() else { return };
        if let Some(first) = stop_beside(self.starts.iter().copied(), end.block, within, way) {
            self.selection.move_to_start_of(TextAt::on_page(first), window, cx);
        }
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
        self.references.drawing(cx);
        let area = ElementId::Name("proposal-page".into());
        let parts = {
            let images = (&self.linked, self.proposal.current.images.as_slice());
            let mut writing = Writing {
                texts: &self.texts,
                selection: &self.selection,
                images,
                references: &self.references,
                area: area.clone(),
                drawn: 0,
            };
            let choosing =
                Choosing { this: cx.weak_entity(), chosen: &self.answering.chosen, standing: self.proposal.standing() };
            let revision = &self.proposal.current;
            // The page is drawn as one block, so its texts are the first of a page that isn't a chat.
            self.texts.drawing(Of::Document(0), || document::parts(revision, &mut writing, &choosing, cx))
        };
        self.starts = parts.iter().map(|part| part.first).collect();
        // Beside the page on a desk, and not on a handheld, which has no room for it. *Not built:* the layout's width
        // deciding, as "Each client" in `docs/spec/look.md` says.
        let outline = (look::Reach::now() == look::Reach::Desk).then(|| self.outline(&parts, cx));
        let page = self.page(parts, area, cx);
        let foot = self.foot(window, cx);
        // Across the top of the page, as a banner is across a pane.
        let banner = self.banner(cx);
        let beside = v_flex().flex_1().min_w_0().h_full().children(banner).child(page).children(foot);
        let page = div().flex().size_full().min_h_0().children(outline).child(beside);
        with_step_keys(self.selection.with_keys(page), cx)
    }
}
