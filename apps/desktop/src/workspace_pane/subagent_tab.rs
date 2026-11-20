//! The workspace's one subagent tab, which shows one subagent's work, as the file tab shows one file. Opening another
//! subagent shows that one in the same tab, so following subagents doesn't leave a trail of tabs. It sits at the far
//! left, just after the file tab, as tabs that are replaced rather than added to usually do, so its band names the
//! chat whose subagent it shows. It closes with that chat, since its agent stops then.
//!
//! Each chat's subagents' records, which the chat draws their calls from and the tab its top, are client-core's, and the
//! pane passes them on to every transcript it has as they change.

use std::sync::Arc;

use client_core::records::{Snapshot, subagent_named};
use gpui_kit::component::{Icon, IconName, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled, Subscription, div,
};
use look::{IconSizing as _, Typeset as _};
use protocol::model::{SessionId, Subagent, SubagentId};
use transcript::{Transcript, TranscriptEvent, shown_kind, subagent_standing};

use super::band::Band;
use super::view::chat_title;
use super::{PaneEvent, Showing, WorkspacePane, tab};

/// The subagent tab, while it's open.
#[derive(Default)]
pub(super) struct Subagents {
    tab: Option<SubagentTab>,
}

struct SubagentTab {
    /// The chat whose subagent it shows.
    session: SessionId,
    subagent: SubagentId,
    /// What the call says the subagent is for, until its record comes.
    title: String,
    transcript: Entity<Transcript>,
    /// Focused while it's in front, so the window's keys, such as `Ctrl-Tab`, still reach the pane.
    focus: FocusHandle,
    /// It covers the active chat.
    in_front: bool,
    _events: Subscription,
}

impl Subagents {
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

    /// The chat whose subagent the tab shows, while it's open.
    pub(super) fn tab_session(&self) -> Option<&SessionId> {
        self.tab.as_ref().map(|tab| &tab.session)
    }

    pub(super) fn focus(&self) -> Option<&FocusHandle> {
        self.tab.as_ref().map(|tab| &tab.focus)
    }

    pub(super) fn transcript(&self) -> Option<&Entity<Transcript>> {
        self.tab.as_ref().map(|tab| &tab.transcript)
    }

    /// The subagent the tab shows, once its record has come.
    fn shown<'a>(&self, records: &'a Snapshot) -> Option<&'a Subagent> {
        let tab = self.tab.as_ref()?;
        records.subagent(&tab.session, &tab.subagent).map(AsRef::as_ref)
    }

    /// What the tab's subagent is for.
    fn title(&self, records: &Snapshot) -> Option<String> {
        let tab = self.tab.as_ref()?;
        Some(self.shown(records).and_then(|s| s.description.clone()).unwrap_or_else(|| tab.title.clone()))
    }
}

impl WorkspacePane {
    /// The subagent the subagent tab shows, while it's open.
    pub fn subagent_tab(&self) -> Option<&SubagentId> {
        self.subagents.tab.as_ref().map(|tab| &tab.subagent)
    }

    /// The subagent tab's transcript, while it's open.
    #[cfg(feature = "testing")]
    pub fn subagent_transcript(&self) -> Option<Entity<Transcript>> {
        self.subagents.tab.as_ref().map(|tab| tab.transcript.clone())
    }

    /// Every transcript the pane has: each chat's, and the subagent tab's.
    pub(super) fn every_transcript(&self) -> impl Iterator<Item = &Entity<Transcript>> {
        let tab = self.subagents.tab.as_ref().map(|tab| &tab.transcript);
        self.chats.values().filter_map(|chat| chat.transcript.as_ref()).map(|(transcript, _)| transcript).chain(tab)
    }

    /// A chat's subagents as the pane last had them, for a transcript to start with.
    pub(super) fn subagents_of(&self, session: &SessionId) -> &[Arc<Subagent>] {
        self.shown.all().subagents(session)
    }

    /// Shows the work of `subagent`, in the chat `session`, in the subagent tab, opening the tab if it isn't, and
    /// brings it to the front. `title` is what its call says the subagent is for.
    pub fn open_subagent_tab(
        &mut self,
        session: SessionId,
        subagent: SubagentId,
        title: String,
        cx: &mut Context<Self>,
    ) {
        // One named by the call that started it is named as its record names it, once that's come.
        let subagent = subagent_named(self.subagents_of(&session), &subagent).map_or(subagent, |named| named.id());
        let tab = self.subagents.tab.as_ref();
        let showing = tab.is_some_and(|tab| tab.session == session && tab.subagent == subagent);
        if !showing && let Some(chat) = self.shown.session(&session).cloned() {
            let (client, records, shown) = (self.client.clone(), self.shown.all().clone(), subagent.clone());
            let feed = self.records.clone();
            let transcript = cx.new(|cx| {
                let mut transcript = Transcript::subagent(client, &chat, shown, cx);
                transcript.follow_records(feed);
                transcript.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx);
                transcript.records_changed(&records, cx);
                transcript
            });
            let chat = session.clone();
            let events = cx.subscribe(&transcript, move |this, _, event: &TranscriptEvent, cx| match event {
                TranscriptEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
                // A subagent its subagent started opens in the same tab.
                TranscriptEvent::OpenSubagent { subagent, title } => {
                    this.open_subagent_tab(chat.clone(), subagent.clone(), title.clone(), cx);
                }
                // As does a proposal it made, in the chat's proposal tab.
                TranscriptEvent::OpenProposal { proposal } => this.open_proposal_tab(chat.clone(), proposal, true, cx),
                // Nothing is sent from it, a subagent's work has no turns, no question is asked in it, since
                // approvals are the agent's own, and nothing's found in it.
                TranscriptEvent::GoToWorkspace { workspace } => cx.emit(PaneEvent::GoToWorkspace(workspace.clone())),
                TranscriptEvent::NotSent { .. }
                | TranscriptEvent::Answering { .. }
                | TranscriptEvent::TurnEnded
                | TranscriptEvent::Asking { .. }
                | TranscriptEvent::FindClosed => {}
            });
            let focus = cx.focus_handle();
            self.subagents.tab =
                Some(SubagentTab { session, subagent, title, transcript, focus, in_front: false, _events: events });
        }
        self.bring_to_front(Showing::Subagent, cx);
    }

    /// Closes the subagent tab, and shows the chat it covered.
    pub fn close_subagent_tab(&mut self, cx: &mut Context<Self>) {
        if self.subagents.tab.take().is_some_and(|tab| tab.in_front) {
            self.bring_to_front(Showing::Chat, cx);
        }
        cx.notify();
    }

    /// Closes the subagent tab once its chat has closed.
    pub(super) fn keep_subagent_tab_valid(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.subagents.tab_session() else { return };
        if !self.open_sessions().any(|s| &s.id == session) {
            self.close_subagent_tab(cx);
        }
    }

    /// Names the tab's subagent as its record does. A tab opened on the subagent a call started, before its record
    /// came, names it by that call, and the hub follows it by the ID its agent gives it once the agent says it started.
    /// Its record names it so, as its transcript does once it has the record.
    pub(super) fn name_the_tab_s_subagent(&mut self) {
        if let Some(tab) = &mut self.subagents.tab
            && let Some(named) = subagent_named(self.shown.all().subagents(&tab.session), &tab.subagent)
        {
            tab.subagent = named.id();
        }
    }

    /// The subagent tab, built as the file tab is. It has the call's icon, and what the subagent is for, in italics,
    /// since it's replaced rather than added to.
    pub(super) fn subagent_tab_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let title = self.subagents.title(self.shown.all())?;
        let selected = self.showing() == Showing::Subagent;
        let close =
            tab::close_button("close-subagent-tab", "Close subagent tab", cx, |t, _, cx| t.close_subagent_tab(cx));
        let tab = look::tab("show-subagent-tab", title.clone(), look::TabWidth::Even)
            .label(format!("Subagent: {title}"))
            .tooltip(title)
            .mark(Icon::new(IconName::Bot).sized(look::IconSize::Large))
            .replaced()
            .showing(selected)
            .closes(close)
            .on_click(cx.listener(|t, _, _, cx| t.bring_to_front(Showing::Subagent, cx)));
        Some(tab.into_any_element())
    }

    /// What shows under the tabs while the subagent tab is in front: a band that says which chat it's from, what the
    /// subagent is and where it stands, as the diff's file header does, then its work, with no composer.
    pub(super) fn subagent_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(tab) = &self.subagents.tab else { return div().into_any_element() };
        v_flex()
            .track_focus(&tab.focus)
            .flex_1()
            .min_h_0()
            .w_full()
            .children(self.subagents.title(self.shown.all()).map(|title| {
                let chat = self.shown.session(&tab.session);
                let from = chat.map(|chat| (chat.id.clone(), chat_title(chat).to_string()));
                band(from, title, self.subagents.shown(self.shown.all()), cx)
            }))
            .child(div().flex_1().min_h_0().w_full().child(Transcript::drawn(&tab.transcript, cx)))
            .into_any_element()
    }
}

/// The chat it's from, which shows that chat when clicked, then what the subagent is for, its kind unless it's the
/// default, and where it stands, once its record has come.
fn band(
    from: Option<(SessionId, String)>,
    title: String,
    subagent: Option<&Subagent>,
    cx: &Context<WorkspacePane>,
) -> AnyElement {
    let colors = look::colors(cx);
    let after = subagent
        .and_then(|s| shown_kind(s.kind.as_deref()))
        .map(|kind| look::chip(look::ChipKind::Subagent, look::ChipSize::Regular, kind.to_string()).into_any_element());
    let end = subagent.map(subagent_standing).map(|standing| {
        div().flex_none().typeset(look::Type::Detail).text_color(colors.text_faint).child(standing).into_any_element()
    });
    Band { id: "subagent", icon: Icon::new(IconName::Bot), from, title, after, end }.element(cx)
}
