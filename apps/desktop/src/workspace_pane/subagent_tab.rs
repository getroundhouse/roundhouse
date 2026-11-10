//! The workspace's one subagent tab, which shows one subagent's work, as the diff tab shows one file. Opening another
//! subagent shows that one in the same tab. A sketch for #417, on a branch that's never merged, which draws each
//! option `transcript::Sketch` names.

use gpui_kit::base::Tab;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Styled, div, px, relative,
};
use look::{Signal, Work};
use protocol::model::{SessionId, ToolUseId};
use transcript::{FocusStatus, FocusSummary, Sketch, Transcript, sketch};

use super::view::{TAB_GROUP, TAB_WIDTH};
use super::{Showing, SubagentTab, WorkspacePane};

impl WorkspacePane {
    /// Shows the work of the subagent `call` started, in the chat `session`, in the subagent tab, opening the tab if
    /// it isn't, and brings it to the front.
    pub fn open_subagent_tab(&mut self, session: SessionId, call: ToolUseId, cx: &mut Context<Self>) {
        let showing = self.subagent_tab.as_ref().is_some_and(|tab| tab.session == session && tab.call == call);
        if !showing && let Some(chat) = self.sessions.iter().find(|s| s.id == session).cloned() {
            let client = self.client.clone();
            let focused = call.clone();
            let transcript = cx.new(|cx| Transcript::focused(client, &chat, focused, cx));
            self.subagent_tab = Some(SubagentTab { session, call, transcript, in_front: false });
        }
        self.bring_to_front(Showing::Subagent, cx);
    }

    /// Closes the subagent tab, and shows the chat it covered.
    pub fn close_subagent_tab(&mut self, cx: &mut Context<Self>) {
        self.subagent_tab = None;
        self.bring_to_front(Showing::Chat, cx);
    }

    fn summary(&self, cx: &Context<Self>) -> Option<FocusSummary> {
        self.subagent_tab.as_ref()?.transcript.read(cx).focus_summary()
    }

    /// The subagent tab, just after the tab of the chat whose subagent it shows, built as the diff tab is.
    pub(super) fn subagent_tab_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let summary = self.summary(cx)?;
        let selected = self.showing() == Showing::Subagent;
        let muted = cx.theme().muted_foreground;
        let close = Button::new("close-subagent-tab")
            .ghost()
            .xsmall()
            .icon(IconName::Close)
            .tooltip("Close subagent tab")
            .on_click(cx.listener(|t, _, _, cx| {
                cx.stop_propagation();
                t.close_subagent_tab(cx);
            }));
        let lead = match sketch() {
            // A subagent is an agent, so it has the mark a chat has, which says whether it's at work.
            Sketch::Chat => look::mark(signal(summary.status), Work::NoPr, look::Size::Inline, cx).into_any_element(),
            Sketch::Page | Sketch::Thread => Icon::new(IconName::Bot).xsmall().into_any_element(),
        };
        let clipped = || div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis();
        let name = match sketch() {
            Sketch::Chat | Sketch::Page => clipped().flex_1().italic().child(summary.title.clone()).into_any_element(),
            // Its kind first, as the call's chip says it, then what it's for, faint.
            Sketch::Thread => h_flex()
                .flex_1()
                .min_w_0()
                .gap_1p5()
                .overflow_hidden()
                .children(summary.kind.clone().map(|kind| div().flex_none().child(kind)))
                .child(clipped().text_color(look::faint()).child(summary.title.clone()))
                .into_any_element(),
        };
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
                    .accessibility_label(format!("Subagent: {}", summary.title))
                    .size_full()
                    .justify_start()
                    .gap_2()
                    .pl_3()
                    .pr_1p5()
                    .text_sm()
                    .line_height(relative(1.25))
                    .text_color(muted)
                    .on_click(cx.listener(|t, _, _, cx| t.bring_to_front(Showing::Subagent, cx)))
                    .child(lead)
                    .child(name)
                    .child(div().flex_none().invisible().group_hover(TAB_GROUP, Styled::visible).child(close)),
            )
            .when(selected, |d| {
                d.child(div().absolute().left_0().right_0().bottom_0().h(px(2.)).bg(look::tab_underline()))
            });
        Some(tab.into_any_element())
    }

    /// What shows under the tabs while the subagent tab is in front: its work, with no composer, and on a page, a
    /// band at its top that says what the subagent is and where it's got to.
    pub(super) fn subagent_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(tab) = &self.subagent_tab else { return div().into_any_element() };
        let band = self.summary(cx).filter(|_| sketch() == Sketch::Page).map(|summary| page_band(&summary, cx));
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .children(band)
            .child(div().flex_1().min_h_0().w_full().child(tab.transcript.clone()))
            .into_any_element()
    }
}

fn signal(status: FocusStatus) -> Signal {
    match status {
        FocusStatus::Working => Signal::Working,
        FocusStatus::Failed => Signal::Failed,
        FocusStatus::Finished | FocusStatus::Stopped => Signal::Idle,
    }
}

/// A page's top, as the diff's file header is: what the subagent is for, its kind, and where it stands.
fn page_band(summary: &FocusSummary, cx: &Context<WorkspacePane>) -> AnyElement {
    let theme = cx.theme();
    let chip = summary.kind.clone().map(|kind| {
        div()
            .px_2()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.input)
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(kind)
    });
    h_flex()
        .flex_none()
        .w_full()
        .h(px(40.))
        .px_4()
        .gap_2()
        .border_b_1()
        .border_color(theme.border)
        .child(Icon::new(IconName::Bot).xsmall().text_color(look::faint()))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(summary.title.clone()))
        .children(chip)
        .child(div().flex_1())
        .child(div().text_xs().text_color(look::faint()).child(summary.standing()))
        .into_any_element()
}
