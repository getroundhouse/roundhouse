//! How a workspace looks: its header, tabs and chat, and the sidebar of changes beside them. The composer is in
//! `composer.rs`.

use gpui_kit::base::{Tab, Tabs};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
    StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div, px, relative,
};
use protocol::model::{Session, SessionStatus};

use super::diff_tab::Sidebar;
use super::setup::setup_log_view;
use super::{HEADER_HEIGHT, Mark, Showing, WorkspacePane};
use crate::right_click;

/// Every tab is the same width, so the row doesn't shift when a chat gets its title.
pub(super) const TAB_WIDTH: Pixels = px(200.);
/// Each tab's, so a tab's close button shows only while the pointer is over that tab.
pub(super) const TAB_GROUP: &str = "chat-tab";

pub(super) fn chat_title(session: &Session, number: usize) -> String {
    session.title.as_deref().map_or_else(|| format!("Chat {number}"), str::to_string)
}

impl WorkspacePane {
    fn chat_tab(&self, session: &Session, title: String, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let selected = self.active.as_ref() == Some(&session.id) && self.showing() == Showing::Chat;
        let unread = self.shows_unread(session);
        let mark = crate::sidebar::signal(Some(session.status), unread);
        let work = self.work();
        let (to_show, to_close) = (session.id.clone(), session.id.clone());
        let close = Button::new(ElementId::Name(format!("close-chat-{}", session.id).into()))
            .ghost()
            .xsmall()
            .icon(IconName::Close)
            .tooltip("Close chat")
            .on_click(cx.listener(move |t, _, window, cx| {
                // The tab under it would take the click too, and show the chat that's closing.
                cx.stop_propagation();
                t.close_chat(to_close.clone(), window, cx);
            }));
        let menu = Self::tab_menu(session, &title, unread, cx);
        let tab = div()
            .id(ElementId::Name(format!("tab-{}", session.id).into()))
            .group(TAB_GROUP)
            .relative()
            .flex_none()
            .w(TAB_WIDTH)
            .h_full()
            .child(
                Tab::new(ElementId::Name(format!("chat-{}", session.id).into()))
                    .selected(selected)
                    .accessibility_label(title.clone())
                    .size_full()
                    .justify_start()
                    .gap_2()
                    .pl_3()
                    .pr_1p5()
                    .text_sm()
                    .line_height(relative(1.25))
                    // Only a reply that hasn't been read stands out. The open chat shows in its underline, so moving
                    // between chats doesn't change how their titles look.
                    .when(!unread, |d| d.text_color(muted))
                    .when(unread, |d| d.font_weight(FontWeight::MEDIUM))
                    .on_click(cx.listener(move |t, _, _, cx| t.activate(to_show.clone(), cx)))
                    .child(look::mark(mark, work, look::Size::Inline, cx))
                    .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(title))
                    // Hidden rather than left out, so the title doesn't move when it shows.
                    .child(div().flex_none().invisible().group_hover(TAB_GROUP, Styled::visible).child(close)),
            )
            .when(selected, |d| {
                d.child(div().absolute().left_0().right_0().bottom_0().h(px(2.)).bg(look::tab_underline()))
            });
        tab.context_menu(menu).into_any_element()
    }

    /// What a right-click on a chat's tab offers. Marking it needs a reply.
    fn tab_menu(
        session: &Session,
        title: &str,
        unread: bool,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let marking = if unread { Some(Mark::Read) } else { (session.reply_seq > 0).then_some(Mark::Unread) };
        let (this, session, title) = (cx.weak_entity(), session.clone(), title.to_string());
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

    /// The diff tab if there is one, then one tab for each open chat, and a way to start another chat. At the far end
    /// are any closed ones to go back to.
    fn chat_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let numbered: Vec<(usize, Session)> = self.sessions.iter().cloned().enumerate().collect();
        let mut tabs: Vec<AnyElement> = self.diff_tab_element(cx).into_iter().collect();
        for (index, session) in numbered.iter().filter(|(_, s)| s.closed_at_ms.is_none()) {
            tabs.push(self.chat_tab(session, chat_title(session, index + 1), cx));
            // The subagent tab sits just after the tab of the chat whose subagent it shows.
            if self.subagent_tab.as_ref().is_some_and(|tab| tab.session == session.id) {
                tabs.extend(self.subagent_tab_element(cx));
            }
        }
        let closed = self.closed_chats(cx);
        div()
            .relative()
            .flex_none()
            .w_full()
            .h(px(40.))
            // The bar's bottom edge, drawn under the tabs rather than as a border, so the line under the chat showing
            // can cover it. GPUI draws a border over what's inside it.
            .child(div().absolute().left_0().right_0().bottom_0().h(px(1.)).bg(border))
            .child(
                h_flex()
                    .size_full()
                    .pl_1()
                    .pr_3()
                    .gap_1()
                    .child(Tabs::new("chat-tabs").flex().h_full().min_w_0().overflow_x_scroll().children(tabs))
                    .child(
                        Button::new("new-chat")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Plus)
                            .tooltip("New chat")
                            .on_click(cx.listener(|t, _, _, cx| t.new_chat(cx))),
                    )
                    // Kept to the end of the row, so it stays put as chats open and close.
                    .child(div().flex_1())
                    .children(closed),
            )
    }

    /// Over the chat, beside the sidebar of changes. While the sidebar is hidden, it shows the pull request's number and
    /// its bar's action instead.
    fn header(&self, sidebar: Sidebar, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let controls = match sidebar {
            Sidebar::Open => None,
            Sidebar::Hidden => {
                let on_action = cx.listener(|t, action, _, cx| t.act(*action, cx));
                crate::pull_request::controls(&self.pull_request, &self.busy, &self.workspace.id, on_action, cx)
            }
        };
        h_flex()
            .flex_none()
            .w_full()
            .h(HEADER_HEIGHT)
            .pl_4()
            .pr_3()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(self.heading(cx))
            .children(controls)
            .child(
                Button::new("toggle-diff")
                    .ghost()
                    .small()
                    .icon(IconName::PanelRight)
                    .tooltip(match sidebar {
                        Sidebar::Open => "Hide diff",
                        Sidebar::Hidden => "Show diff",
                    })
                    .on_click(cx.listener(|t, _, _, cx| t.toggle_changes(cx))),
            )
    }

    /// What the workspace is called, and the branch it's on and where that started.
    fn heading(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (mono, muted) = (theme.mono_font_family.clone(), theme.muted_foreground);
        let title = self.workspace.label().to_string();
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .overflow_hidden()
            .whitespace_nowrap()
            .child(
                div()
                    .id("workspace-title")
                    .role(gpui_kit::Role::Heading)
                    .aria_label(title.clone())
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(title)
                    .test_support(),
            )
            .child(div().text_color(look::faint()).child("/"))
            .child(div().text_xs().font_family(mono).text_color(muted).child(self.workspace.branch.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(look::faint())
                    .child(format!("from {}", self.workspace.base_ref.as_deref().unwrap_or("?"))),
            )
    }
}

impl Render for WorkspacePane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let changes = self.changes_sidebar(cx);
        // Hidden too while there's no diff to show in it yet, so the header shows the pull request meanwhile.
        let sidebar = if changes.is_some() { Sidebar::Open } else { Sidebar::Hidden };
        let header = self.header(sidebar, cx).into_any_element();
        let tabs = self.chat_tabs(cx).into_any_element();
        let file = self.diff.as_ref().filter(|_| self.showing() == Showing::Diff).map(|diff| diff.file.clone());
        let showing = match file {
            Some(file) => div().flex_1().min_h_0().w_full().child(file).into_any_element(),
            None if self.showing() == Showing::Subagent => self.subagent_view(cx),
            None => self.chat(cx),
        };
        h_flex()
            .size_full()
            .child(v_flex().flex_1().min_w_0().h_full().child(header).child(tabs).child(showing))
            .children(changes)
    }
}

impl WorkspacePane {
    /// The active chat, under the tabs: its transcript, how setup is going, and the composer.
    fn chat(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let interrupted = self.session().is_some_and(|s| s.status == SessionStatus::Interrupted);
        let setup_line = self.setup_line(cx);
        let composer = self.composer_row(cx).into_any_element();
        let log = self.setup_log.clone();
        let transcript = self.transcript();
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(div().flex_1().min_h_0().w_full().when_some(transcript, ParentElement::child))
            .when_some(log, |d, text| d.child(setup_log_view(text, cx)))
            .children(setup_line)
            // In the transcript's column, so it starts where the rows above it do.
            .when(interrupted, |d| {
                d.child(
                    transcript::in_column(
                        "The agent stopped before it finished. Send a message to pick up where it left off.",
                    )
                    .flex_none()
                    .py_1()
                    .text_xs()
                    .text_color(muted),
                )
            })
            .child(composer)
            .into_any_element()
    }
}
