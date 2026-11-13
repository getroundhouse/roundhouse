//! The workspace's one subagent tab, which shows one subagent's work, as the file tab shows one file. Opening another
//! subagent shows that one in the same tab, so following subagents doesn't leave a trail of tabs. It sits at the far
//! left, just after the file tab, as tabs that are replaced rather than added to usually do, so its band names the
//! chat whose subagent it shows. It closes with that chat, since its agent stops then.
//!
//! Each chat's subagents' records, which the chat draws their calls from and the tab its top, are client-core's, and the
//! pane passes them on as they change.

use std::sync::Arc;

use client_core::records::{Changes, Snapshot};
use gpui_kit::base::Tab;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled, Subscription, TestSupportExt as _, div, px,
    relative,
};
use look::{Hand as _, Shaped as _, Sizing as _, Typeset as _};
use protocol::model::{SessionId, Subagent, ToolUseId};
use transcript::{Transcript, TranscriptEvent, shown_kind, subagent_standing};

use super::view::{TAB_GROUP, TAB_WIDTH, chat_title};
use super::{PaneEvent, Showing, WorkspacePane};

/// The subagent tab, while it's open.
#[derive(Default)]
pub(super) struct Subagents {
    tab: Option<SubagentTab>,
}

struct SubagentTab {
    /// The chat whose subagent it shows.
    session: SessionId,
    call: ToolUseId,
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

    /// The subagent the tab shows, once its record has come.
    fn shown<'a>(&self, records: &'a Snapshot) -> Option<&'a Subagent> {
        let tab = self.tab.as_ref()?;
        records.subagent(&tab.session, &tab.call).map(AsRef::as_ref)
    }

    /// What the tab's subagent is for.
    fn title(&self, records: &Snapshot) -> Option<String> {
        let tab = self.tab.as_ref()?;
        Some(self.shown(records).and_then(|s| s.description.clone()).unwrap_or_else(|| tab.title.clone()))
    }
}

impl WorkspacePane {
    /// The call that started the subagent the subagent tab shows, while it's open.
    pub fn subagent_tab(&self) -> Option<&ToolUseId> {
        self.subagents.tab.as_ref().map(|tab| &tab.call)
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

    /// Shows the work of the subagent `call` started, in the chat `session`, in the subagent tab, opening the tab if
    /// it isn't, and brings it to the front. `title` is what the call says the subagent is for.
    pub fn open_subagent_tab(&mut self, session: SessionId, call: ToolUseId, title: String, cx: &mut Context<Self>) {
        let tab = self.subagents.tab.as_ref();
        let showing = tab.is_some_and(|tab| tab.session == session && tab.call == call);
        if !showing && let Some(chat) = self.shown.session(&session).cloned() {
            let (client, records, shown) = (self.client.clone(), self.subagents_of(&session).to_vec(), call.clone());
            let transcript = cx.new(|cx| {
                let mut transcript = Transcript::subagent(client, &chat, shown, cx);
                transcript.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx);
                transcript.set_subagents(&records, cx);
                transcript
            });
            let chat = session.clone();
            let events = cx.subscribe(&transcript, move |this, _, event: &TranscriptEvent, cx| match event {
                TranscriptEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
                // A subagent its subagent started opens in the same tab.
                TranscriptEvent::OpenSubagent { call, title } => {
                    this.open_subagent_tab(chat.clone(), call.clone(), title.clone(), cx);
                }
                // Nothing is sent from it, a subagent's work has no turns, no question is asked in it, since
                // approvals are the agent's own, and nothing's found in it.
                TranscriptEvent::NotSent { .. }
                | TranscriptEvent::Answering { .. }
                | TranscriptEvent::TurnEnded
                | TranscriptEvent::Asking { .. }
                | TranscriptEvent::FindClosed => {}
            });
            let focus = cx.focus_handle();
            self.subagents.tab =
                Some(SubagentTab { session, call, title, transcript, focus, in_front: false, _events: events });
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

    /// Passes each chat's subagents on to its transcript, and to the tab if it shows one of them, when they've changed
    /// since the pane last looked, as `changes` says.
    pub(super) fn pass_on_subagents(&self, changes: &Changes<'_>, cx: &mut Context<Self>) {
        for (id, chat) in &self.chats {
            let Some((transcript, _)) = &chat.transcript else { continue };
            if !changes.subagents(id) {
                continue;
            }
            let list = self.subagents_of(id);
            transcript.update(cx, |t, cx| t.set_subagents(list, cx));
            if let Some(tab) = self.subagents.tab.as_ref().filter(|tab| &tab.session == id) {
                tab.transcript.update(cx, |t, cx| t.set_subagents(list, cx));
            }
        }
    }

    /// The subagent tab, built as the file tab is. It has the call's icon, and what the subagent is for, in italics,
    /// since it's replaced rather than added to.
    pub(super) fn subagent_tab_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = look::colors(cx);
        let title = self.subagents.title(self.shown.all())?;
        let selected = self.showing() == Showing::Subagent;
        let muted = colors.text_muted;
        let close = Button::new("close-subagent-tab")
            .ghost()
            .sized_square(look::ControlSize::Small)
            .icon(IconName::Close)
            .tooltip("Close subagent tab")
            .on_click(cx.listener(|t, _, _, cx| {
                // The tab under it would take the click too, and show the subagent that's closing.
                cx.stop_propagation();
                t.close_subagent_tab(cx);
            }));
        let name = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().italic();
        let whole = title.clone();
        let tab = div()
            .id("subagent-tab")
            .group(TAB_GROUP)
            .relative()
            .flex_none()
            .w(TAB_WIDTH)
            .h_full()
            .child(
                Tab::new("show-subagent-tab")
                    .selected(selected)
                    .accessibility_label(format!("Subagent: {title}"))
                    .tooltip(move |window, cx| Tooltip::new(whole.clone()).build(window, cx))
                    .size_full()
                    .justify_start()
                    .gap_2()
                    .pl_3()
                    .pr_1p5()
                    .typeset(look::Type::Body)
                    .line_height(relative(1.25))
                    .text_color(muted)
                    .on_click(cx.listener(|t, _, _, cx| t.bring_to_front(Showing::Subagent, cx)))
                    .child(Icon::new(IconName::Bot).xsmall())
                    .child(name.child(title))
                    .child(div().flex_none().invisible().group_hover(TAB_GROUP, Styled::visible).child(close)),
            )
            .when(selected, |d| {
                d.child(div().absolute().left_0().right_0().bottom_0().h(look::TAB_LINE).bg(colors.current))
            });
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
            .child(div().flex_1().min_h_0().w_full().child(tab.transcript.clone()))
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
    let from = from.map(|(chat, name)| {
        let link = div()
            .id("subagent-band-chat")
            .min_w_0()
            .max_w(px(240.))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .typeset(look::Type::Body)
            .text_color(colors.text_faint)
            .hand()
            .hover(|d| d.text_color(colors.text))
            .role(Role::Link)
            .aria_label(name.clone())
            .on_click(cx.listener(move |t, _, _, cx| t.activate(chat.clone(), cx)))
            .child(name)
            .test_support();
        // It gives way well before what the subagent is for does, which is what the page is about.
        let slash = div().flex_none().typeset(look::Type::Body).text_color(colors.text_faint).child("/");
        h_flex().flex_shrink(100.).min_w(px(56.)).gap_2().child(link).child(slash)
    });
    let chip = subagent.and_then(|s| shown_kind(s.kind.as_deref())).map(|kind| {
        div()
            .flex_none()
            .px_2()
            .corners(look::Corner::Control)
            .border_1()
            .border_color(colors.control_edge)
            .typeset(look::Type::Body)
            .text_color(colors.text_muted)
            .child(kind.to_string())
    });
    let standing = subagent.map(subagent_standing);
    h_flex()
        .flex_none()
        .w_full()
        .h(px(40.))
        .px_4()
        .gap_2()
        .border_b_1()
        .border_color(colors.edge)
        .child(Icon::new(IconName::Bot).xsmall().text_color(colors.text_faint))
        .children(from)
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .typeset(look::Type::Body)
                .emphasis()
                .child(title),
        )
        .children(chip)
        .child(div().flex_1())
        .children(
            standing.map(|s| div().flex_none().typeset(look::Type::Detail).text_color(colors.text_faint).child(s)),
        )
        .into_any_element()
}
