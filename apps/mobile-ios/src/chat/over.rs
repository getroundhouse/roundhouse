//! What a line in the chat opens over it, as a screen of its own with a way back to the chat, where the desktop opens
//! each in a tab beside the chat: a subagent's work, which nothing is typed into, or a proposal, to answer. Opening
//! another shows it in place of the one open, as the desktop's tab does, so following subagents leaves no trail of
//! screens to go back through. Going back to the chat keeps it, as the desktop's tab stays open behind the chat, so an
//! answer chosen and not sent is there when the proposal's line is tapped again.

use client_core::Client;
use client_core::records::{Snapshot, subagent_named};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    Subscription, Window, div,
};
use protocol::model::{ProposalId, SubagentId};
use transcript::{ProposalEvent, ProposalView, Transcript, TranscriptEvent, shown_kind, subagent_standing};

use super::view::{ChatEvent, ChatView, PROSE_LINE, read};
use crate::chrome;

/// The screen a line in the chat last opened, if any, and whether it's in front of the chat.
#[derive(Default)]
pub(super) enum Over {
    #[default]
    Nothing,
    InFront(Screen),
    /// Kept behind the chat after going back to it, so it's as it was if it's opened again.
    Behind(Screen),
}

pub(super) enum Screen {
    Subagent(SubagentScreen),
    Proposal(ProposalScreen),
}

pub(super) struct SubagentScreen {
    subagent: SubagentId,
    /// What its call says the subagent is for, until its record comes.
    title: String,
    transcript: Entity<Transcript>,
    _events: Subscription,
}

pub(super) struct ProposalScreen {
    view: Entity<ProposalView>,
    _answers: Subscription,
}

impl Over {
    /// Whether a screen is open over the chat, and in front of it.
    pub(super) fn in_front(&self) -> bool {
        matches!(self, Over::InFront(_))
    }

    /// The screen open over the chat, in front of it or behind.
    fn screen(&self) -> Option<&Screen> {
        match self {
            Over::InFront(screen) | Over::Behind(screen) => Some(screen),
            Over::Nothing => None,
        }
    }

    fn screen_mut(&mut self) -> Option<&mut Screen> {
        match self {
            Over::InFront(screen) | Over::Behind(screen) => Some(screen),
            Over::Nothing => None,
        }
    }

    /// The screen that's open, brought in front of the chat.
    fn bring_in_front(&mut self) {
        *self = match std::mem::take(self) {
            Over::InFront(screen) | Over::Behind(screen) => Over::InFront(screen),
            Over::Nothing => Over::Nothing,
        };
    }

    /// The screen that's open, put behind the chat.
    fn put_behind(&mut self) {
        *self = match std::mem::take(self) {
            Over::InFront(screen) | Over::Behind(screen) => Over::Behind(screen),
            Over::Nothing => Over::Nothing,
        };
    }
}

impl ChatView {
    /// Shows the work of `subagent`, one of the shown chat's, over the chat. `title` is what its call says the subagent
    /// is for.
    pub(super) fn open_subagent(&mut self, subagent: SubagentId, title: String, cx: &mut Context<Self>) {
        let Some(shown) = &self.shown else { return };
        let records = self.feed.snapshot();
        // One named by the call that started it is named as its record names it, once that's come.
        let subagent =
            subagent_named(records.subagents(&shown.session.id), &subagent).map_or(subagent, |named| named.id());
        let open = matches!(self.over.screen(), Some(Screen::Subagent(screen)) if screen.subagent == subagent);
        if !open {
            let (client, feed, session) = (self.client.clone(), self.feed.clone(), shown.session.clone());
            let id = subagent.clone();
            let transcript = cx.new(|cx| {
                let mut transcript = Transcript::subagent(client, &session, id, cx);
                transcript.follow_records(feed);
                transcript.records_changed(&records, cx);
                transcript
            });
            let events = cx.subscribe(&transcript, |chat, _, event: &TranscriptEvent, cx| match event {
                TranscriptEvent::Error(message) => chat.say(message, cx),
                // A subagent its subagent started shows in its place, as does a proposal it made.
                TranscriptEvent::OpenSubagent { subagent, title } => {
                    chat.open_subagent(subagent.clone(), title.clone(), cx);
                }
                TranscriptEvent::OpenProposal { proposal } => chat.open_proposal(proposal, cx),
                TranscriptEvent::GoToWorkspace { workspace } => cx.emit(ChatEvent::GoToWorkspace(workspace.clone())),
                // Nothing is sent from it, a subagent's work has no turns, no question is asked in it, since approvals
                // are the agent's own, and nothing's found in it.
                TranscriptEvent::NotSent { .. }
                | TranscriptEvent::Answering { .. }
                | TranscriptEvent::TurnEnded
                | TranscriptEvent::Asking { .. }
                | TranscriptEvent::FindClosed => {}
            });
            self.over = Over::Behind(Screen::Subagent(SubagentScreen { subagent, title, transcript, _events: events }));
        }
        self.over.bring_in_front();
        self.read_showing(cx);
        cx.notify();
    }

    /// Shows `proposal`, one of the shown chat's, over the chat, once its record has come, since until then there's
    /// nothing to show.
    pub(super) fn open_proposal(&mut self, proposal: &ProposalId, cx: &mut Context<Self>) {
        let Some(shown) = &self.shown else { return };
        let open = matches!(self.over.screen(), Some(Screen::Proposal(screen)) if screen.view.read(cx).proposal().id == *proposal);
        if !open {
            let Some(record) = self.feed.snapshot().proposal(&shown.session.id, proposal).cloned() else { return };
            let client = self.client.clone();
            let view = cx.new(|cx| ProposalView::new(record, client, cx));
            view.read(cx).follow_records(self.feed.clone());
            let answers = cx.subscribe(&view, |chat, _, event: &ProposalEvent, cx| match event {
                ProposalEvent::Error(message) => chat.say(message, cx),
                ProposalEvent::GoToWorkspace(workspace) => cx.emit(ChatEvent::GoToWorkspace(workspace.clone())),
            });
            self.over = Over::Behind(Screen::Proposal(ProposalScreen { view, _answers: answers }));
        }
        self.over.bring_in_front();
        self.read_showing(cx);
        cx.notify();
    }

    /// Carries what's open over the chat on over a new connection.
    pub(super) fn reconnect_over(&self, client: &Client, cx: &mut Context<Self>) {
        match self.over.screen() {
            Some(Screen::Subagent(screen)) => {
                screen.transcript.update(cx, |transcript, cx| transcript.reconnect(client, cx));
            }
            Some(Screen::Proposal(screen)) => screen.view.update(cx, |view, cx| view.reconnect(client, cx)),
            None => {}
        }
    }

    /// Passes the chat's subagents and proposals on to what's open over it.
    pub(super) fn over_records_changed(&mut self, records: &Snapshot, cx: &mut Context<Self>) {
        let Some(session) = self.session().cloned() else { return };
        match self.over.screen_mut() {
            Some(Screen::Subagent(screen)) => {
                // The hub follows one opened by the call that started it by the ID its agent gives it, once the agent
                // says it started.
                if let Some(named) = subagent_named(records.subagents(&session), &screen.subagent) {
                    screen.subagent = named.id();
                }
                screen.transcript.update(cx, |transcript, cx| transcript.records_changed(records, cx));
            }
            Some(Screen::Proposal(screen)) => {
                let id = screen.view.read(cx).proposal().id.clone();
                if let Some(newer) = records.proposal(&session, &id) {
                    screen.view.update(cx, |view, cx| view.set_proposal(newer.clone(), cx));
                }
            }
            None => {}
        }
    }

    /// The screen over the chat, while one's in front: a bar that says what it is, with a way back to the chat, then
    /// what it shows.
    pub(super) fn over_screen(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Over::InFront(screen) = &self.over else { return None };
        let back = chrome::back().on_click(cx.listener(|chat, _, _, cx| {
            chat.over.put_behind();
            chat.read_showing(cx);
            cx.notify();
        }));
        let (title, under, body) = match screen {
            Screen::Subagent(screen) => {
                let records = self.feed.snapshot();
                let record =
                    self.session().and_then(|session| subagent_named(records.subagents(session), &screen.subagent));
                // What it's for, then its kind unless it's the default, and where it stands, as the desktop's band
                // says them, once its record has come.
                let title = record.and_then(|s| s.description.clone()).unwrap_or_else(|| screen.title.clone());
                let under = record.map(|s| match shown_kind(s.kind.as_deref()) {
                    Some(kind) => format!("{kind} · {}", subagent_standing(s)),
                    None => subagent_standing(s),
                });
                (title, under, read(&screen.transcript, cx))
            }
            Screen::Proposal(screen) => {
                let title = screen.view.read(cx).proposal().current.proposed.title.trim().to_string();
                let body = div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .line_height(PROSE_LINE)
                    .child(screen.view.clone())
                    .into_any_element();
                (title, Some("Proposal".to_string()), body)
            }
        };
        let bottom = crate::safe_area::insets(window, cx).effective().bottom;
        let header = chrome::header(title.into(), under.map(SharedString::from), Some(back), Vec::new(), window, cx);
        let screen = v_flex()
            .size_full()
            .pb(bottom)
            .child(header)
            .when_some(self.error.clone(), |d, error| d.child(chrome::error(error)))
            .child(body);
        Some(screen.into_any_element())
    }

    /// The work of the subagent shown over the chat, while it's in front.
    #[cfg(feature = "testing")]
    pub(crate) fn subagent_transcript(&self) -> Option<Entity<Transcript>> {
        match &self.over {
            Over::InFront(Screen::Subagent(screen)) => Some(screen.transcript.clone()),
            Over::InFront(Screen::Proposal(_)) | Over::Behind(_) | Over::Nothing => None,
        }
    }
}
