//! The panel of a sign-in with GitHub, as the hub's record of it stands: while it waits, the code to type, with a
//! button that copies it, and one that opens where to type it, and then how it ended.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, TestSupportExt as _, div,
};
use look::{Shaped as _, Typeset as _};
use protocol::model::GithubUserCode;

use super::Drawing;
use super::kept::Panel;
use super::panel::{Framed, button, buttons, framed, waiting_line};
use super::sign_in::SignInPanel;
use super::sign_in_words::{self, Stands};

pub(super) fn sign_in(drawing: &Drawing<'_>, panel: &Panel, sign_in: &SignInPanel, cx: &App) -> AnyElement {
    let stands = Stands::of(sign_in, drawing.records, drawing.github, cx);
    let body = if let Stands::Waiting { code, address, until } = &stands {
        Some(waiting(code, address, until.as_deref(), cx))
    } else {
        None
    };
    let framed_as = Framed {
        title: stands.title().into(),
        lead: stands.lead(sign_in.again.as_deref()).into(),
        body,
        refused: None,
        buttons: buttons_for(drawing, &stands, sign_in.again.clone(), cx),
    };
    framed(drawing, panel, framed_as, cx)
}

/// Cancel while the hub may still wait for the person, and otherwise Close, or Done once it's approved, with Sign in
/// again for one that ended in a way another try might change.
fn buttons_for(drawing: &Drawing<'_>, stands: &Stands<'_>, again: Option<String>, cx: &App) -> gpui_kit::Div {
    let kept = drawing.kept.clone();
    if stands.may_wait() {
        return buttons().child(button("git-cancel", "Cancel", false, false, move |window, cx| {
            kept.update(cx, |kept, cx| kept.cancel_sign_in(window, cx));
        }));
    }
    let approved = matches!(stands, Stands::Approved { .. });
    let close = button("git-close", if approved { "Done" } else { "Close" }, approved, false, move |window, cx| {
        kept.update(cx, |kept, cx| kept.put_away_panel(window, cx));
    });
    let try_again = stands.may_try_again().then(|| {
        let kept = drawing.kept.clone();
        button("git-sign-in-again", "Sign in again", true, !drawing.can_ask(cx), move |_, cx| {
            kept.update(cx, |kept, cx| kept.sign_in(again.clone(), cx));
        })
    });
    buttons().child(close).children(try_again)
}

/// The code to type, in code's font, with a button that copies it, beside one that opens `address`, where it's typed,
/// and under them, that it waits, `until` when the code works.
fn waiting(code: &GithubUserCode, address: &str, until: Option<&str>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let code = h_flex()
        .pl(look::Space::Medium)
        .pr(look::Space::XSmall)
        .py(look::Space::XSmall)
        .gap(look::Space::Small)
        .corners(look::Corner::Control)
        .border(look::EDGE)
        .border_color(colors.edge)
        .bg(colors.inset)
        .child(
            div()
                .id("git-code")
                .typeset_code(cx)
                .emphasis()
                .text_color(colors.text)
                .child(code.to_string())
                .test_support(),
        )
        .child(look::copy_button("git-copy-code", code.to_string(), "Copy code", cx));
    let address = address.to_string();
    let open = look::button(
        "git-open-address",
        look::ButtonKind::Outlined,
        look::ControlSize::Regular,
        format!("Open {}", address.trim_start_matches("https://")),
    )
    .icon(Lucide::ExternalLink)
    .on_click(move |_, _, cx| cx.open_url(&address));
    let line = waiting_line(sign_in_words::waiting(until), cx);
    v_flex()
        .gap(look::Space::Large)
        .child(h_flex().items_center().gap(look::Space::Medium).child(code).child(open))
        .child(line)
        .into_any_element()
}
