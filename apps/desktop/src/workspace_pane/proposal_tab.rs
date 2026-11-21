//! The workspace's one proposal tab, which shows one proposal from one of its chats, as the subagent tab shows one
//! subagent's work: opening another shows that one in the same tab. It sits with the other tabs that are replaced,
//! after the subagent tab, and closes with the chat it's from. It opens by itself, without coming to the front, for a
//! proposal the pane hasn't seen, so one waiting for the person is a tab away without taking them from what they're
//! doing, unless it's in front already, showing another, or holds an answer they haven't sent. Open, on the proposal's
//! line in its chat, brings it to the front, on its newest revision, which its view holds back while the person reads
//! the one before it. The answer, or the request for another round, its view asks to send goes to the hub from here,
//! and the view is told how it went. Its note is dictated into only while the tab is in front, as `dictating` has the
//! composer's.
//!
//! Each chat's proposals are client-core's, which the pane follows with its subagents, and passes on as they change.

use std::collections::HashSet;
use std::sync::Arc;

use client_core::records::Changes;
use gpui_kit::component::{Icon, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled, Subscription, div,
};
use look::{IconSizing as _, InRegion as _, Region};
use protocol::model::{Proposal, ProposalId, SessionId};
use transcript::{PROPOSAL_ICON, ProposalEvent, ProposalRequest, ProposalView, Way};

use super::band::Band;
use super::view::chat_title;
use super::{PaneEvent, Showing, WorkspacePane, tab};
use crate::{SelectStepBack, SelectStepOn, StepBack, StepOn};

/// The proposal tab, while it's open, and which proposals the pane has seen.
#[derive(Default)]
pub(super) struct Proposals {
    tab: Option<ProposalTab>,
    /// Each proposal the pane has been given, so each opens the tab by itself only once.
    seen: HashSet<ProposalId>,
}

struct ProposalTab {
    /// The chat whose proposal it shows.
    session: SessionId,
    view: Entity<ProposalView>,
    /// It covers the active chat.
    in_front: bool,
    _answers: Subscription,
}

impl Proposals {
    /// Whether the tab is open and covers the active chat.
    pub(super) fn in_front(&self) -> bool {
        self.tab.as_ref().is_some_and(|tab| tab.in_front)
    }

    /// Puts the tab in front, or behind.
    pub(super) fn set_in_front(&mut self, in_front: bool) {
        if let Some(tab) = &mut self.tab {
            tab.in_front = in_front;
        }
    }

    /// The chat whose proposal the tab shows, while it's open.
    pub(super) fn tab_session(&self) -> Option<&SessionId> {
        self.tab.as_ref().map(|tab| &tab.session)
    }

    pub(super) fn view(&self) -> Option<&Entity<ProposalView>> {
        self.tab.as_ref().map(|tab| &tab.view)
    }

    /// Takes the tab away, ending what its note was dictating, which a view that's gone can't do.
    fn take(&mut self, cx: &mut App) -> Option<ProposalTab> {
        let tab = self.tab.take()?;
        tab.view.update(cx, |view, cx| view.set_in_sight(false, cx));
        Some(tab)
    }
}

impl WorkspacePane {
    /// The proposal the proposal tab shows, while it's open.
    pub fn proposal_tab(&self, cx: &App) -> Option<ProposalId> {
        Some(self.proposals.view()?.read(cx).proposal().id.clone())
    }

    /// Whether the pane has taken in the record of `proposal`, of the chat `session`, for a test to wait for, since
    /// the pane takes records in after the feed has them, and decides then whether the tab shows it by itself.
    #[cfg(any(test, feature = "testing"))]
    pub fn has_proposal(&self, session: &SessionId, proposal: &ProposalId) -> bool {
        self.shown.all().proposal(session, proposal).is_some()
    }

    /// Shows `proposal`, of the chat `session`, in the proposal tab, opening the tab if it isn't, and brings it to the
    /// front if `to_front`, on its newest revision. Until the pane has its record there's nothing to show, so the tab stays as it is, rather
    /// than coming to the front with another proposal in it.
    pub fn open_proposal_tab(
        &mut self,
        session: SessionId,
        proposal: &ProposalId,
        to_front: bool,
        cx: &mut Context<Self>,
    ) {
        let showing = self.proposal_tab(cx).as_ref() == Some(proposal);
        if !showing {
            let Some(record) = self.shown.all().proposal(&session, proposal).cloned() else { return };
            let in_front = self.proposals.in_front();
            let client = self.client.clone();
            let view = cx.new(|cx| ProposalView::new(record, client, cx));
            let pane = cx.weak_entity();
            let chat = session.clone();
            view.update(cx, |view, _| {
                view.set_dictation_hints(move |collector, cx| {
                    if let Some(pane) = pane.upgrade() {
                        pane.read(cx).collect_chat_hints(&chat, collector, cx);
                    }
                });
            });
            view.read(cx).follow_records(self.records.clone());
            let answers = cx.subscribe(&view, |this, view, event: &ProposalEvent, cx| match event {
                ProposalEvent::Send(request) => this.answer_proposal(view, request.clone(), cx),
                ProposalEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
                ProposalEvent::GoToWorkspace(workspace) => cx.emit(PaneEvent::GoToWorkspace(workspace.clone())),
            });
            self.proposals.take(cx);
            self.proposals.tab = Some(ProposalTab { session, view, in_front, _answers: answers });
        }
        if to_front {
            // Open on a line in the chat asks for the proposal as it is now, so a revision the tab held back shows.
            if let Some(view) = self.proposals.view() {
                view.update(cx, ProposalView::show_newest);
            }
            self.bring_to_front(Showing::Proposal, cx);
        }
        cx.notify();
    }

    /// Sends the person's answer, or their request for another round, to the hub, which tells the agent, and tells
    /// `view` how it went.
    fn answer_proposal(&mut self, view: Entity<ProposalView>, request: ProposalRequest, cx: &mut Context<Self>) {
        let doing = match &request {
            ProposalRequest::Answer(_) => "send your answer",
            ProposalRequest::AnotherRound(_) => "ask for another round",
        };
        let answering = self.client.start(move |client| match request {
            ProposalRequest::Answer(answer) => client.proposal_answer(answer),
            ProposalRequest::AnotherRound(round) => client.proposal_another_round(round),
        });
        // client-core takes in the answered proposal the reply carries, so the copy has it by the time this runs.
        cx.spawn(async move |_, cx| {
            let outcome = match answering.recv().await {
                Ok(_) => Ok(()),
                Err(e) => Err(client_core::refusals::couldnt(doing, &e)),
            };
            view.update(cx, |view, cx| view.sent(outcome, cx));
        })
        .detach();
    }

    /// Closes the proposal tab, and shows the chat it covered.
    pub fn close_proposal_tab(&mut self, cx: &mut Context<Self>) {
        if self.proposals.take(cx).is_some_and(|tab| tab.in_front) {
            self.bring_to_front(Showing::Chat, cx);
        }
        cx.notify();
    }

    /// Closes the proposal tab once its chat has closed.
    pub(super) fn keep_proposal_tab_valid(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.proposals.tab_session() else { return };
        if !self.open_sessions().any(|s| &s.id == session) {
            self.close_proposal_tab(cx);
        }
    }

    /// Passes each chat's proposals on to the tab if it shows one of them, when they've changed since the pane last
    /// looked, as `changes` says, and opens the tab by itself for the newest the pane hasn't seen. The pane passes them
    /// on to the transcripts with the rest of their records.
    pub(super) fn pass_on_proposals(&mut self, changes: &Changes<'_>, cx: &mut Context<Self>) {
        let mut unseen: Option<Arc<Proposal>> = None;
        for (id, chat) in &self.chats {
            if chat.transcript.is_none() || !changes.proposals(id) {
                continue;
            }
            let list = self.shown.all().proposals(id);
            if let Some(view) = self.proposals.view() {
                let shown = view.read(cx).proposal().id.clone();
                if let Some(newer) = list.iter().find(|proposal| proposal.id == shown) {
                    view.update(cx, |view, cx| view.set_proposal(newer.clone(), cx));
                }
            }
            for proposal in list.iter().filter(|proposal| self.proposals.seen.insert(proposal.id.clone())) {
                unseen =
                    unseen.filter(|newest| newest.created_at_ms > proposal.created_at_ms).or(Some(proposal.clone()));
            }
        }
        // Not over what the person has chosen or written in the tab and not sent, which showing another would lose.
        let unsent = self.proposals.view().is_some_and(|view| view.read(cx).has_unsent(cx));
        if let Some(proposal) = unseen.filter(|_| !self.proposals.in_front() && !unsent) {
            self.open_proposal_tab(proposal.session_id.clone(), &proposal.id, false, cx);
        }
    }

    /// The proposal tab, named for its proposal's title.
    pub(super) fn proposal_tab_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let title = self.proposals.view()?.read(cx).proposal().current.proposed.title.trim().to_string();
        let selected = self.showing() == Showing::Proposal;
        let close =
            tab::close_button("close-proposal-tab", "Close proposal tab", cx, |t, _, cx| t.close_proposal_tab(cx));
        let tab = look::tab("show-proposal-tab", title.clone(), look::TabWidth::Even)
            .label(format!("Proposal: {title}"))
            .tooltip(title)
            .mark(Icon::new(PROPOSAL_ICON).sized(look::IconSize::Large))
            .replaced()
            .showing(selected)
            .closes(close)
            .on_click(cx.listener(|t, _, _, cx| t.bring_to_front(Showing::Proposal, cx)));
        Some(tab.into_any_element())
    }

    /// What shows under the tabs while the proposal tab is in front: a band that says which chat it's from and what
    /// it's about, as the subagent tab's does, then the proposal.
    pub(super) fn proposal_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(tab) = &self.proposals.tab else { return div().into_any_element() };
        let proposal = tab.view.read(cx).proposal().clone();
        let from = self.shown.session(&tab.session).map(|chat| (chat.id.clone(), chat_title(chat).to_string()));
        let view = tab.view.clone();
        let (back, on, select_back, select_on) = (view.clone(), view.clone(), view.clone(), view);
        v_flex()
            .region(Region::Proposal)
            .flex_1()
            .min_h_0()
            .w_full()
            // The page steps between the parts of its outline, and moves the end of what's selected to one with Shift.
            .on_action(move |_: &StepBack, _, cx| back.update(cx, |proposal, cx| proposal.step(Way::Back, cx)))
            .on_action(move |_: &StepOn, _, cx| on.update(cx, |proposal, cx| proposal.step(Way::On, cx)))
            .on_action(move |_: &SelectStepBack, window, cx| {
                select_back.update(cx, |proposal, cx| proposal.select_step(Way::Back, window, cx));
            })
            .on_action(move |_: &SelectStepOn, window, cx| {
                select_on.update(cx, |proposal, cx| proposal.select_step(Way::On, window, cx));
            })
            .child(
                Band {
                    id: "proposal",
                    icon: Icon::new(PROPOSAL_ICON),
                    from,
                    title: proposal.current.proposed.title.trim().to_string(),
                    after: None,
                    end: None,
                }
                .element(cx),
            )
            .child(div().flex_1().min_h_0().w_full().child(tab.view.clone()))
            .into_any_element()
    }

    /// What the proposal tab shows, for a test to read its note.
    #[cfg(any(test, feature = "testing"))]
    pub fn proposal_tab_view(&self) -> Option<Entity<ProposalView>> {
        self.proposals.view().cloned()
    }

    /// Focuses the proposal, so a copy and a select all come to it, and the window's keys still reach the pane.
    pub(super) fn focus_proposal(&self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        if let Some(view) = self.proposals.view() {
            window.focus(&view.focus_handle(cx), cx);
        }
    }
}
