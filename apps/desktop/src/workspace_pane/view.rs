//! How a workspace looks: its header, tabs and chat, and the sidebar of changes beside them. The composer is in
//! `composer.rs`.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::base::Tabs;
use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, App, Context, ElementId, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div,
};
use look::{InRegion as _, LineText, Region, Typeset as _};
use protocol::model::{Session, SessionStatus};

use super::setup::setup_log_view;
use super::workspace_card;
use super::{Mark, Opened, PaneEvent, Showing, WorkspacePane, reserving, tab};
use crate::sides::{self, AtTheTop, Side, Sidebar};
use crate::{NewChat, NewTerminal, ToggleDiff, prompt_cache, right_click};

/// What a chat is called until it has a title.
const UNTITLED: &str = "Untitled";
/// What the `+` after the tabs is called, for whoever can't see it, and in its tooltip.
const NEW_TAB_LABEL: &str = "New chat or terminal";

pub(super) fn chat_title(session: &Session) -> &str {
    session.title.as_deref().unwrap_or(UNTITLED)
}

impl WorkspacePane {
    fn chat_tab(&self, session: &Session, cx: &mut Context<Self>) -> AnyElement {
        let muted = look::colors(cx).text_muted;
        let title = chat_title(session).to_string();
        let selected = self.active.as_ref() == Some(&session.id) && self.showing() == Showing::Chat;
        let unread = self.shows_unread(session);
        let needs_a_look = unread || transcript::waiting_on_you([session]);
        let mark = transcript::chat_signal(session);
        let expiring = prompt_cache::expiring_in_ms(session, look::now_ms(cx))
            .map(|left_ms| prompt_cache::clock(format!("tab-cache-{}", session.id), left_ms, muted));
        let (to_show, to_close) = (session.id.clone(), session.id.clone());
        let close_id = ElementId::Name(format!("close-chat-{}", session.id).into());
        let close = tab::close_button(close_id, "Close chat", cx, move |t, window, cx| {
            t.close_chat(to_close.clone(), window, cx);
        });
        let menu = Self::tab_menu(session, unread, cx);
        // Only a chat that needs a look stands out: one with a reply that hasn't been read, or a question waiting on an
        // answer. The open chat shows in its underline, so moving between chats doesn't change how their titles look.
        look::tab(format!("chat-{}", session.id), title, look::TabWidth::Even)
            .mark(look::chat_mark(mark, look::Size::Inline, cx))
            .when_some(expiring, look::Tab::after)
            .needs_a_look(needs_a_look)
            .showing(selected)
            .closes(close)
            .on_click(cx.listener(move |t, _, _, cx| t.activate(to_show.clone(), cx)))
            .right_click_menu(menu)
            .into_any_element()
    }

    /// What a right-click on a chat's tab offers. Marking it needs a reply.
    fn tab_menu(
        session: &Session,
        unread: bool,
        cx: &mut Context<Self>,
    ) -> impl Fn(look::Menu, &mut Window, &mut App) -> look::Menu + 'static {
        let marking = if unread { Some(Mark::Read) } else { session.has_reply().then_some(Mark::Unread) };
        // Renaming a chat with no title starts from nothing, so pressing Return doesn't make "Untitled" a title typed
        // by hand, which no generated one would ever replace.
        let title = session.title.as_deref().unwrap_or_default().to_string();
        let (this, session) = (cx.weak_entity(), session.clone());
        let (to_mark, id) = (this.clone(), session.id.clone());
        let marking = marking.map(|marking| {
            let mark = move |cx: &mut App| {
                let _ = to_mark.update(cx, |t, cx| t.mark(&session, marking, cx));
            };
            (marking, mark)
        });
        // No key: the one for marking acts on the workspace showing, as its row's menu does.
        right_click::menu(marking, None, move |window, cx| {
            let _ = this.update(cx, |t, cx| t.start_renaming_chat(id.clone(), &title, window, cx));
        })
    }

    /// The file tab, the subagent tab and the proposal tab if they're open, at the far left as tabs that are replaced
    /// rather than added to usually are, then one tab for each open chat and each open terminal, and a menu that starts
    /// another of either. At the far end are any closed chats to go back to.
    fn tab_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tabs: Vec<AnyElement> = self.file_tab_element(cx).into_iter().collect();
        tabs.extend(self.subagent_tab_element(cx));
        tabs.extend(self.proposal_tab_element(cx));
        for session in self.open_sessions() {
            tabs.push(self.chat_tab(session, cx));
        }
        tabs.extend(self.terminal_tabs(cx));
        let closed = self.closed_chats(cx);
        look::bar(look::BarKind::Tabs, look::BarSize::Regular)
            .region(Region::TabBar)
            .child(Tabs::new("chat-tabs").flex().h_full().min_w_0().overflow_x_scroll().children(tabs))
            .child(self.new_tab_menu(cx))
            // Kept to the end of the row, so it stays put as chats open and close.
            .when_some(closed, look::Bar::end)
    }

    /// The `+` after the tabs, and its menu: a new chat, or one with each agent when the hub has more than one, and a
    /// new terminal. New chat and New terminal are the app's actions, so each shows its key and does what its key does.
    fn new_tab_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let agents: Vec<_> = self.records.snapshot().agents().named().map(|a| (a.kind, a.name.clone())).collect();
        let this = cx.weak_entity();
        look::icon_button("new-tab", look::ButtonKind::Borderless, look::ControlSize::Regular, IconName::Plus)
            .accessibility_label(NEW_TAB_LABEL)
            .tooltip(NEW_TAB_LABEL)
            .menu(Anchor::TopLeft, move |mut menu: look::Menu, _, _| {
                let chat = |label: String| look::menu_row(label).icon(Lucide::MessageSquarePlus);
                if agents.len() < 2 {
                    menu = menu.row(chat("New chat".into()).key(Box::new(NewChat)));
                } else {
                    for (kind, name) in &agents {
                        let (kind, this) = (*kind, this.clone());
                        menu = menu.row(chat(format!("New {name} chat")).on_choose(move |_, cx| {
                            let _ = this.update(cx, |t, cx| t.new_chat_with(kind, cx));
                        }));
                    }
                }
                menu.row(look::menu_row("New terminal").icon(Lucide::Terminal).key(Box::new(NewTerminal)))
            })
    }

    /// Over the chat, beside the sidebar of changes. While the sidebar is hidden, it shows the pull request's number and
    /// its bar's action instead. It's in the strip along the window's top, clear of the window's buttons at both ends.
    fn header(&self, sidebar: Sidebar, line_text: &LineText, cx: &mut Context<Self>) -> impl IntoElement {
        let clear = sides::clearance(AtTheTop::Header, cx);
        let controls = match sidebar {
            Sidebar::Open => None,
            Sidebar::Hidden => {
                let on_action = cx.listener(|t, action, _, cx| t.act(*action, cx));
                let doing = crate::busy::doing(self.workspace().status);
                crate::pull_request::controls(&self.pull_request, &self.busy, doing, on_action)
            }
        };
        look::bar(look::BarKind::Words, look::BarSize::Regular)
            .region(Region::Header)
            .clear(clear.start, clear.end)
            .child(self.heading(line_text, cx))
            .children(controls)
            .child(
                look::icon_button(
                    "toggle-diff",
                    look::ButtonKind::Borderless,
                    look::ControlSize::Regular,
                    IconName::PanelRight,
                )
                .tooltip_with_action(
                    match sidebar {
                        Sidebar::Open => "Hide diff",
                        Sidebar::Hidden => "Show diff",
                    },
                    &ToggleDiff,
                    None,
                )
                .on_click(cx.listener(|t, _, _, cx| t.toggle_changes(cx))),
            )
    }

    /// What the workspace is called, and the branch it's on and where that started. When they don't fit, the words
    /// give way and end in an ellipsis, so the button beside the title is never pushed out of view. The branch and
    /// where it started are smaller, and the branch is in the code font, so they're set on the title's baseline, as
    /// words on one line are, rather than centred, which would leave them sitting higher. Resting the pointer on the
    /// title shows where the workspace is, as `workspace_card.rs` says.
    fn heading(&self, line_text: &LineText, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let (muted, faint) = (colors.text_muted, colors.text_faint);
        let LineText { body, detail, code } = line_text;
        let words = || div().min_w_0().overflow_hidden().text_ellipsis();
        let title = self.workspace().label().to_string();
        let title = words().id("workspace-title").role(gpui_kit::Role::Heading).aria_label(title.clone()).child(title);
        let title = body.set(title).emphasis().test_support();
        let title = workspace_card::card(title, self.workspace(), self.client.hub_version());
        let branch = words().text_color(muted).child(self.workspace().branch.to_string());
        let base = words().text_color(faint);
        let base = base.child(format!("from {}", self.workspace().base_ref.as_deref().unwrap_or("?")));
        h_flex()
            .flex_1()
            .min_w_0()
            .gap(look::Space::Small)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(title)
            .child(div().flex_none().child(self.regenerate_title_button(cx)))
            .child(body.set(div().flex_none().text_color(faint).child("/")))
            .child(code.set(branch))
            .child(detail.set(base))
    }

    /// Titles the workspace again from what's been asked in it, beside the title it replaces. It spins while the hub
    /// waits on the model, and is greyed out until there's a title, which the first prompt gives. A workspace renamed
    /// by hand before that has one with nothing to make another from, and the hub says so.
    fn regenerate_title_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.workspace().id.clone();
        look::icon_button(
            "regenerate-title",
            look::ButtonKind::Quiet,
            look::ControlSize::Regular,
            Lucide::PencilSparkles,
        )
        .tooltip("Regenerate title")
        .waiting(self.retitling)
        .off(!self.retitling && self.workspace().title.is_none())
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(PaneEvent::RegenerateTitle(id.clone()))))
    }
}

impl Render for WorkspacePane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.keep_focus_drawn(window, cx);
        // Once for the frame, for everything in the pane that mixes text.
        let line_text = LineText::new(window, cx);
        let changes = self.changes_sidebar(&line_text, cx);
        // What the header says goes by what's chosen, from the moment it is, rather than by how far the sidebar has
        // slid. Hidden too while there's no diff to show in it yet, so the header shows the pull request meanwhile.
        let sidebar = if self.diff.is_some() { Side::Right.chosen(cx).sidebar } else { Sidebar::Hidden };
        let header = self.header(sidebar, &line_text, cx).into_any_element();
        let tabs = self.tab_row(cx).into_any_element();
        let tab =
            self.diff.as_ref().filter(|_| self.showing() == Showing::File).map(|diff| match self.file_tab_opened() {
                Some(Opened::File) => diff.viewer.clone().into_any_element(),
                Some(Opened::Diff) | None => diff.file.clone().into_any_element(),
            });
        let showing = match tab {
            Some(tab) => div().region(Region::OpenFile).flex_1().min_h_0().w_full().child(tab).into_any_element(),
            None if self.showing() == Showing::Subagent => self.subagent_view(cx),
            None if self.showing() == Showing::Proposal => self.proposal_view(cx),
            None => match self.terminal_view() {
                Some(terminal) => terminal,
                None => self.chat(cx),
            },
        };
        h_flex()
            .size_full()
            .child(v_flex().flex_1().min_w_0().h_full().child(header).child(tabs).child(showing))
            .children(changes)
    }
}

impl WorkspacePane {
    /// The active chat, under the tabs: its transcript, how setup is going, whether agents can push, and the composer.
    fn chat(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = look::colors(cx);
        let (muted, faint) = (colors.text_muted, colors.text_faint);
        // A chat that needs an account says so in its card, which says what to do instead.
        let interrupted = self.session().is_some_and(|s| s.status == SessionStatus::Interrupted && !s.needs_an_account);
        let cache_expired = self.session().and_then(|s| prompt_cache::expired_line(s, look::now_ms(cx)));
        let setup_line = self.setup_line(cx);
        let push_line = self.push_line(cx);
        let usage = self.usage(cx);
        let reserve_line = reserving::reserve_line(usage.as_ref(), cx);
        let account_wait = self.account_wait(cx);
        let composer = self.composer_row(cx).into_any_element();
        let log = self.setup_log.clone();
        let transcript = self.transcript();
        let chat = v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when_some(transcript, |d, t| d.child(transcript::Transcript::drawn(&t, cx))),
            )
            .when_some(log, |d, text| d.child(setup_log_view(text, cx)))
            .children(setup_line)
            .children(account_wait)
            .children(push_line)
            .children(reserve_line)
            // In the transcript's column, so it starts where the rows above it do.
            .when(interrupted, |d| {
                d.child(
                    transcript::in_column(
                        "The agent stopped before it finished. Send a message to pick up where it left off.",
                    )
                    .flex_none()
                    .py(look::Space::XSmall)
                    .typeset(look::Type::Detail)
                    .text_color(muted),
                )
            })
            // What the next prompt costs, with a clock as the newest turn's line has.
            .when_some(cache_expired, |d, said| {
                let clock = look::cache_mark(look::CacheMark::Clock, faint);
                let line = h_flex().gap(look::Space::Small).child(clock).child(said);
                d.child(
                    transcript::in_column(line)
                        .flex_none()
                        .py(look::Space::XSmall)
                        .typeset(look::Type::Detail)
                        .text_color(muted),
                )
            })
            .child(composer);
        Self::with_chat_keys(chat, cx).into_any_element()
    }
}
