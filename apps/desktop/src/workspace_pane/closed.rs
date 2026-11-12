//! The chats that were closed, to go back to: a button at the end of the row of tabs, and the menu it opens, which
//! lists them the most recently closed first, with how long ago each was closed.

use std::cmp::Reverse;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement, Pixels, SharedString,
    Styled, Window, div, px,
};
use look::Hand as _;
use protocol::model::SessionId;

use super::WorkspacePane;
use super::view::chat_title;

/// Room for a title of a few words beside how long ago, which a tab's width doesn't leave. It's the row's rather than
/// the menu's, because the kit lets a row of an app's own grow to its whole title instead of shortening it to fit
/// the menu, and the menu fits itself around its rows.
const ROW_WIDTH: Pixels = px(280.);
/// A little taller than the kit's own menu items, so a row with an icon at its end isn't cramped.
const ROW_HEIGHT: Pixels = px(28.);
/// The button's, so its icon lights up wherever the pointer is over it, not only over the icon.
const BUTTON_GROUP: &str = "closed-chats";

/// Whether the menu of closed chats is open, which keeps its button lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Menu {
    Shut,
    Open,
}

struct ClosedChat {
    id: SessionId,
    title: SharedString,
    closed_at_ms: i64,
}

impl WorkspacePane {
    /// Nothing while no chat is closed. The menu is made each time it opens, so how long ago is as of then.
    pub(super) fn closed_chats(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let mut closed: Vec<ClosedChat> = self
            .sessions()
            .iter()
            .filter_map(|session| {
                let closed_at_ms = session.closed_at_ms?;
                Some(ClosedChat { id: session.id.clone(), title: chat_title(session).to_string().into(), closed_at_ms })
            })
            .collect();
        if closed.is_empty() {
            return None;
        }
        closed.sort_by_key(|chat| Reverse(chat.closed_at_ms));
        let theme = cx.theme();
        let (lit, muted) = (theme.foreground, theme.muted_foreground);
        let resting = match self.closed_menu {
            Menu::Open => lit,
            Menu::Shut => muted,
        };
        // Lit by its icon rather than a fill, while the pointer is over it or its menu is open.
        let icon = div()
            .text_color(resting)
            .group_hover(BUTTON_GROUP, move |s| s.text_color(lit))
            .child(Icon::new(IconName::RotateCcwClock).xsmall());
        let (this, on_open) = (cx.weak_entity(), cx.weak_entity());
        let button = Button::new("closed-chats")
            .hand()
            .custom(ButtonCustomVariant::new(cx))
            .xsmall()
            .child(icon)
            .accessibility_label("Closed chats")
            .tooltip("Closed chats")
            // From its right edge, since the button is at the end of the row.
            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, cx| {
                let now_ms = look::now_ms(cx);
                for chat in &closed {
                    let (this, id) = (this.clone(), chat.id.clone());
                    let row = row(chat.title.clone(), crate::ago::ago(chat.closed_at_ms, now_ms).into());
                    menu = menu.item(PopupMenuItem::element(row).on_click(move |_, _, cx| {
                        let _ = this.update(cx, |t, cx| t.reopen_chat(id.clone(), cx));
                    }));
                }
                menu
            })
            .on_open_change(move |open, _, cx| {
                let menu = if *open { Menu::Open } else { Menu::Shut };
                let _ = on_open.update(cx, |t, cx| {
                    t.closed_menu = menu;
                    cx.notify();
                });
            });
        Some(div().group(BUTTON_GROUP).child(button))
    }
}

/// A closed chat in the menu: its title, how long ago it was closed, and an icon saying a click reopens it. It has no
/// mark, because a mark says what a chat's agent is doing, and closing a chat stops its agent.
fn row(title: SharedString, ago: SharedString) -> impl Fn(&mut Window, &mut App) -> AnyElement {
    move |_, cx| {
        let muted = cx.theme().muted_foreground;
        h_flex()
            .w(ROW_WIDTH)
            .h(ROW_HEIGHT)
            .gap_2()
            .child(div().flex_1().min_w_0().truncate().child(title.clone()))
            .child(div().flex_none().text_color(muted).child(ago.clone()))
            .child(Icon::new(IconName::RotateCcw).xsmall().text_color(muted))
            .into_any_element()
    }
}
