//! The band along the top of what a replaced tab shows, the subagent tab or the proposal tab, about one thing from one
//! of the workspace's chats.

use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::{Hand as _, IconSizing as _, Typeset as _};
use protocol::model::SessionId;

use super::WorkspacePane;

/// Its icon, the chat it's from, faint, before a slash, which shows that chat when clicked, then what it is, `after`,
/// such as what kind of subagent it is, and `end` at the bar's end, such as where the subagent stands.
pub(super) struct Band {
    /// What its link to the chat is known by, `{id}-band-chat`.
    pub(super) id: &'static str,
    pub(super) icon: Icon,
    /// The chat it's from, and what that's called, when the pane has it.
    pub(super) from: Option<(SessionId, String)>,
    pub(super) title: String,
    pub(super) after: Option<AnyElement>,
    pub(super) end: Option<AnyElement>,
}

impl Band {
    pub(super) fn element(self, cx: &Context<WorkspacePane>) -> AnyElement {
        let colors = look::colors(cx);
        let from = self.from.map(|(chat, name)| {
            let link = div()
                .id(SharedString::from(format!("{}-band-chat", self.id)))
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
            // It gives way well before what it's about does, which is what the page is about.
            let slash = div().flex_none().typeset(look::Type::Body).text_color(colors.text_faint).child("/");
            h_flex().flex_shrink(100.).min_w(px(56.)).gap(look::Space::Small).child(link).child(slash)
        });
        let title = div()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .typeset(look::Type::Body)
            .emphasis()
            .child(self.title);
        look::bar(look::BarKind::Words, look::BarSize::Regular)
            .child(self.icon.sized(look::IconSize::Large).text_color(colors.text_faint))
            .children(from)
            .child(title)
            .children(self.after)
            .when_some(self.end, look::Bar::end)
            .into_any_element()
    }
}
