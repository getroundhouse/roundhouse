//! A panel of Git's, raised as Agents' panels are: its title and what it says under that, what's in it, then its
//! buttons. Whichever it is, it's brought into sight as it opens, wherever it is on the page, and gives Settings the
//! focus back as it goes.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use look::Typeset as _;

use super::Drawing;
use super::kept::Panel;

/// What a panel says and holds.
pub(super) struct Framed {
    pub(super) title: SharedString,
    pub(super) lead: SharedString,
    pub(super) body: Option<AnyElement>,
    /// Why the hub refused what the panel asked, in red, as an alert, so assistive apps say it as it comes.
    pub(super) refused: Option<String>,
    pub(super) buttons: Div,
}

/// `panel`, as `framed` says.
pub(super) fn framed(drawing: &Drawing<'_>, panel: &Panel, framed: Framed, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let refused = framed.refused.map(|refused| {
        let refused = SharedString::from(refused);
        let said = div().id("git-refused").role(Role::Alert).aria_label(refused.clone());
        said.typeset(look::Type::Body).text_color(colors.error).child(refused).test_support()
    });
    let (kept, scroll) = (drawing.kept.clone(), drawing.scroll.clone());
    look::raised(cx)
        .id("git-panel")
        .track_focus(&panel.focus)
        .relative()
        .flex()
        .flex_col()
        .gap(look::Space::Large)
        .p(look::Space::XLarge)
        .typeset(look::Type::Body)
        .child(
            v_flex()
                .gap(look::Space::XSmall)
                .child(div().emphasis().child(framed.title))
                .child(said_under_the_title(framed.lead, cx)),
        )
        .children(framed.body)
        .children(refused)
        .child(framed.buttons)
        .on_prepaint(move |bounds, window, cx| kept.update(cx, |kept, _| kept.laid_out(bounds, &scroll, window)))
        .test_support()
        .into_any_element()
}

/// What a panel says under its title, which tests read by `git-panel-says`, as assistive apps do.
fn said_under_the_title(said: SharedString, cx: &App) -> impl IntoElement + use<> {
    let said_as = div().id("git-panel-says").role(Role::Label).aria_label(said.clone());
    said_as.text_color(look::colors(cx).text_muted).child(said).test_support()
}

/// A panel's buttons, at its end.
pub(super) fn buttons() -> Div {
    h_flex().justify_end().gap(look::Space::Small)
}

/// One of a panel's buttons, known to tests by `id`: its main one when it's `primary`, which can be `off`.
pub(super) fn button(
    id: &'static str,
    label: &'static str,
    primary: bool,
    off: bool,
    act: impl Fn(&mut Window, &mut App) + 'static,
) -> look::Button {
    let kind = if primary { look::ButtonKind::Main } else { look::ButtonKind::Outlined };
    look::button(id, kind, look::ControlSize::Regular, label)
        .off(off)
        .on_click(move |_: &ClickEvent, window, cx| act(window, cx))
}
