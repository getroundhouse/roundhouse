//! How a workspace looks: its header, tabs, composer and pickers.

use gpui_kit::base::{Tab, Tabs};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
    StatefulInteractiveElement, Styled, Window, div, px, relative,
};
use protocol::model::{Session, SessionId, SessionStatus};
use transcript::mode_label;

use super::setup::setup_log_view;
use super::{HEADER_HEIGHT, Mark, PaneEvent, WorkspacePane, in_turn};
use crate::StopAgent;

/// Aliases, so these stay the newest of each without a release here. `default` is the account's own.
pub(super) const MODELS: [(&str, &str); 5] =
    [("default", "Default model"), ("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];

const MODES: [&str; 4] = ["manual", "plan", "acceptEdits", "bypassPermissions"];

pub(super) fn model_label(model: Option<&str>) -> String {
    let model = model.unwrap_or("default");
    MODELS.iter().find(|(id, _)| *id == model).map_or(model, |(_, label)| label).to_string()
}

/// Every tab is the same width, so the row doesn't shift when a chat gets its title.
const TAB_WIDTH: Pixels = px(200.);
/// Each tab's, so a tab's close button shows only while the pointer is over that tab.
const TAB_GROUP: &str = "chat-tab";

fn chat_title(session: &Session, number: usize) -> String {
    session.title.clone().unwrap_or_else(|| format!("Chat {number}"))
}

impl WorkspacePane {
    fn chat_tab(&self, session: &Session, title: String, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let selected = self.active.as_ref() == Some(&session.id);
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
                    // A reply that hasn't been read stands out as the selected chat's title does.
                    .when(!selected && !unread, |d| d.text_color(muted))
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
        let marking = if unread { Some(Mark::Read) } else { (session.reply_seq > 0).then_some(Mark::Unread) };
        let Some(marking) = marking else { return tab.into_any_element() };
        let (this, session) = (cx.weak_entity(), session.clone());
        tab.context_menu(move |menu, _, _| {
            let (this, session) = (this.clone(), session.clone());
            menu.item(PopupMenuItem::new(format!("Mark as {}", marking.word())).on_click(move |_, _, cx| {
                let _ = this.update(cx, |t, cx| t.mark(&session, marking, cx));
            }))
        })
        .into_any_element()
    }

    /// One tab for each open chat, then a way to start another, and any closed ones to go back to.
    fn chat_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let numbered: Vec<(usize, Session)> = self.sessions.iter().cloned().enumerate().collect();
        let tabs: Vec<AnyElement> = numbered
            .iter()
            .filter(|(_, s)| s.closed_at_ms.is_none())
            .map(|(index, session)| self.chat_tab(session, chat_title(session, index + 1), cx))
            .collect();
        let closed: Vec<(SessionId, String)> = numbered
            .iter()
            .filter(|(_, s)| s.closed_at_ms.is_some())
            .map(|(index, s)| (s.id.clone(), chat_title(s, index + 1)))
            .collect();
        let this = cx.weak_entity();
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
                    .when(!closed.is_empty(), |d| {
                        d.child(
                            Button::new("closed-chats")
                                .ghost()
                                .xsmall()
                                .label(format!("Closed ({})", closed.len()))
                                .dropdown_menu(move |mut menu, _, _| {
                                    for (id, title) in &closed {
                                        let (this, id) = (this.clone(), id.clone());
                                        menu =
                                            menu.item(PopupMenuItem::new(title.clone()).on_click(move |_, _, cx| {
                                                let _ = this.update(cx, |t, cx| t.reopen_chat(id.clone(), cx));
                                            }));
                                    }
                                    menu
                                }),
                        )
                    }),
            )
    }

    /// The model and approval mode of the chat showing.
    fn pickers(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let model = session.and_then(|s| s.model.clone());
        let mode = session.and_then(|s| s.permission_mode.clone());
        let full_access = mode.as_deref() == Some("bypassPermissions");
        let warning = cx.theme().warning;
        let mode_text = mode.as_deref().map_or("Your settings", mode_label).to_string();
        let this = cx.weak_entity();
        let models = {
            let (this, current) = (this.clone(), model.clone().unwrap_or_else(|| "default".to_string()));
            move |mut menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                for (id, label) in MODELS {
                    let this = this.clone();
                    menu = menu.item(PopupMenuItem::new(label).checked(current == id).on_click(move |_, _, cx| {
                        let _ = this.update(cx, |t, cx| t.configure(Some(id.to_string()), None, cx));
                    }));
                }
                menu
            }
        };
        let approval_modes = {
            let current = mode.clone();
            move |mut menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                for id in MODES {
                    let this = this.clone();
                    let checked = current.as_deref() == Some(id);
                    menu = menu.item(PopupMenuItem::new(mode_label(id).to_string()).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = this.update(cx, |t, cx| t.configure(None, Some(id.to_string()), cx));
                        },
                    ));
                }
                menu
            }
        };
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(
                Button::new("model-picker")
                    .ghost()
                    .xsmall()
                    .dropdown_caret(true)
                    .label(model_label(model.as_deref()))
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, models),
            )
            .child(
                Button::new("mode-picker")
                    .ghost()
                    .xsmall()
                    .dropdown_caret(true)
                    .when(full_access, |b| b.text_color(warning))
                    .label(mode_text)
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, approval_modes),
            )
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mono = theme.mono_font_family.clone();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let id = self.workspace.id.clone();
        let pr = self.pull_request.clone();
        h_flex()
            .flex_none()
            .w_full()
            .h(HEADER_HEIGHT)
            .px_4()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(self.workspace.name.clone()))
                    .child(div().text_color(look::faint()).child("/"))
                    .child(div().text_xs().font_family(mono).text_color(muted).child(self.workspace.branch.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(look::faint())
                            .child(format!("from {}", self.workspace.base_ref.as_deref().unwrap_or("?"))),
                    ),
            )
            .child(
                Button::new("toggle-diff")
                    .ghost()
                    .small()
                    .icon(IconName::PanelRight)
                    .label(if self.diff_open { "Hide diff" } else { "Diff" })
                    .on_click(cx.listener(|t, _, _, cx| t.toggle_diff(cx))),
            )
            .child(Button::new("archive").ghost().small().label("Archive").on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(PaneEvent::Archive(id.clone(), false));
            })))
            .when_some(pr, |d, pr| {
                let url = pr.url.clone();
                d.child(
                    Button::new("pull-request")
                        .small()
                        .icon(IconName::Github)
                        .label(format!("#{} · {}", pr.number, self.work().name()))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
            .when(self.pull_request.is_none(), |d| {
                d.child(
                    Button::new("create-pr")
                        .primary()
                        .small()
                        .label("Create PR")
                        .on_click(cx.listener(|t, _, _, cx| t.create_pull_request(cx))),
                )
            })
    }

    fn composer_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pickers = self.pickers(cx).into_any_element();
        let working = self.session().is_some_and(in_turn);
        let theme = cx.theme();
        let (surface, border) = (theme.muted, theme.input);
        div().flex_none().w_full().px_4().pb_4().pt_1().child(
            v_flex()
                .w_full()
                .rounded_lg()
                .border_1()
                .border_color(border)
                .bg(surface)
                .child(div().w_full().px_1().pt_1().child(Textarea::new(&self.composer).appearance(false)))
                .child(h_flex().w_full().px_2().pb_2().gap_2().child(pickers).child(if working {
                    Button::new("stop")
                        .small()
                        .icon(IconName::Square)
                        .label("Stop")
                        .tooltip_with_action("Stop", &StopAgent, None)
                        .on_click(cx.listener(|t, _, _, cx| t.stop_agent(cx)))
                        .into_any_element()
                } else {
                    Button::new("send")
                        .primary()
                        .small()
                        .icon(IconName::ArrowUp)
                        .tooltip("Send")
                        .on_click(cx.listener(|t, _, window, cx| t.submit(window, cx)))
                        .into_any_element()
                })),
        )
    }
}

impl Render for WorkspacePane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (border, muted) = (cx.theme().border, cx.theme().muted_foreground);
        let interrupted = self.session().is_some_and(|s| s.status == SessionStatus::Interrupted);
        let setup_line = self.setup_line(cx);
        let header = self.header(cx).into_any_element();
        let tabs = self.chat_tabs(cx).into_any_element();
        let composer = self.composer_row(cx).into_any_element();
        let log = self.setup_log.clone();
        let transcript = self.transcript();
        let diff = if self.diff_open { self.diff.clone() } else { None };
        v_flex().size_full().child(header).child(
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(tabs)
                        .child(div().flex_1().min_h_0().w_full().when_some(transcript, ParentElement::child))
                        .when_some(log, |d, text| d.child(setup_log_view(text, cx)))
                        .children(setup_line)
                        .when(interrupted, |d| {
                            d.child(div().flex_none().w_full().px_4().py_1().text_xs().text_color(muted).child(
                                "The agent stopped before it finished. Send a message to pick up where it left off.",
                            ))
                        })
                        .child(composer),
                )
                .when_some(diff, |d, diff| {
                    d.child(div().flex_1().min_w_0().h_full().border_l_1().border_color(border).child(diff))
                }),
        )
    }
}
