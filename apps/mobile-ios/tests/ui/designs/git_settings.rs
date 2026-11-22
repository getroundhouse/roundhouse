//! Settings' Git page: the GitHub accounts agents push and open pull requests as, and connecting one.

#![cfg(test)]

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, Hsla, IntoElement, ParentElement as _, Styled as _, div, px};
use look::{
    BadgeKind, ButtonKind, ControlSize, Corner, IconSize, Lined as _, RowSize, Shaped as _, Space, Type, Typeset as _,
};

use super::canvas::Canvas;
use super::parts;
use super::settings::page;

pub(super) fn draw(canvas: &mut Canvas) {
    canvas.draw("settings-git", |_, cx| git(None, cx));
    canvas.draw("settings-connect-sheet", |_, cx| git(Some(connect_sheet(cx)), cx));
    canvas.draw("settings-signing-in", |_, cx| signing_in(cx));
}

/// A GitHub account agents push as: its picture, its name over how it was connected, and how its login stands.
fn github(
    id: &'static str,
    name: &'static str,
    default: bool,
    made: &'static str,
    stands: &'static str,
    tone: Option<Hsla>,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let picture = div()
        .flex_none()
        .size(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(colors.raised)
        .child(parts::icon(IconName::Github, IconSize::Large, colors.text_muted));
    let named = h_flex()
        .gap(Space::Small)
        .child(div().min_w_0().truncate().emphasis().child(name))
        .children(default.then(|| look::badge(BadgeKind::Plain, "Default")));
    let said = h_flex()
        .gap(Space::XSmall)
        .typeset(Type::Detail)
        .text_color(tone.unwrap_or(colors.text_muted))
        .children(tone.map(|tone| parts::icon(IconName::TriangleAlert, IconSize::Small, tone)))
        .child(format!("{made} · {stands}"));
    h_flex()
        .w_full()
        .gap(Space::Medium)
        .px(Space::Medium)
        .py(Space::Medium)
        .line_under(cx)
        .child(picture)
        .child(v_flex().flex_1().min_w_0().gap(Space::XSmall).child(named).child(said))
        .child(look::icon_button(id, ButtonKind::Borderless, ControlSize::Large, IconName::Ellipsis))
        .into_any_element()
}

fn git(over: Option<AnyElement>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let connect =
        look::button("connect", ButtonKind::Outlined, ControlSize::Small, "Connect account").icon(IconName::Plus);
    let make_default = look::button("machines-own", ButtonKind::Borderless, ControlSize::Small, "Make default");
    let found = |name: &'static str, says: &'static str| {
        h_flex()
            .w_full()
            .gap(Space::Medium)
            .px(Space::Medium)
            .py(Space::Medium)
            .line_under(cx)
            .child(parts::icon(IconName::Laptop, IconSize::Large, colors.text_muted))
            .child(div().flex_1().child(name))
            .child(div().typeset(Type::Detail).text_color(colors.text_muted).child(says))
    };
    parts::screen(cx)
        .child(page("Git"))
        .child(parts::heading("Added", Some(connect.into_any_element()), cx).px(Space::Medium).pt(Space::Medium))
        .child(parts::said("Every machine’s git and gh use the default for github.com.", cx))
        .child(github("github-1", "ada-l", true, "Signed in", "Renews by itself", None, cx))
        .child(github("github-2", "roundhouse-ada", false, "App", "Installed on 3 repositories", None, cx))
        .child(github("github-3", "ada-ci", false, "Token", "Ended Oct 29, 2026", Some(colors.error), cx))
        .child(parts::heading("Found", Some(make_default.into_any_element()), cx).px(Space::Medium).pt(Space::Large))
        .child(parts::said("The gh login each machine already has.", cx))
        .child(found("This Mac", "ada-l"))
        .child(found("build-box", "Not logged in"))
        .children(over)
        .into_any_element()
}

/// The ways to connect a GitHub account, which Connect account opens.
fn connect_sheet(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let way = |id: &'static str, glyph: IconName, name: &'static str, does: &'static str| {
        look::list_row(id, RowSize::Large)
            .child(parts::icon(glyph, IconSize::Large, colors.text_muted))
            .child(parts::names(name, does, cx))
            .opens()
    };
    let which = look::button("which", ButtonKind::Borderless, ControlSize::Small, "Which should I choose?");
    look::sheet()
        .title("Connect a GitHub account")
        .clear(parts::HOME_INDICATOR)
        .child(
            parts::rows()
                .py(look::ROWS_INSET)
                .child(way("sign-in", IconName::Github, "Sign in with GitHub", "Agents act as you"))
                .child(way("token", IconName::KeyRound, "Use a token", "Agents act as you, within the token"))
                .child(way("app", IconName::Bot, "Make an app of your own", "Agents act as the app’s bot"))
                .child(h_flex().px(Space::Small).pt(Space::Small).child(which)),
        )
        .into_any_element()
}

/// A sign-in that waits to be approved on GitHub, whichever client started it.
fn signing_in(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let code = h_flex()
        .gap(Space::Medium)
        .justify_center()
        .py(Space::Large)
        .corners(Corner::Card)
        .bg(colors.inset)
        .border(look::EDGE)
        .border_color(colors.edge)
        .child(div().typeset_code(cx).text_size(px(28.)).child("WDJB-MJHT"))
        .child(look::icon_button("copy", ButtonKind::Borderless, ControlSize::Large, IconName::Copy));
    let open = look::button("open", ButtonKind::Main, ControlSize::Large, "Open github.com/login/device").full_width();
    let cancel = look::button("cancel", ButtonKind::Borderless, ControlSize::Large, "Cancel").full_width();
    let waiting = div()
        .text_center()
        .typeset(Type::Detail)
        .text_color(colors.text_muted)
        .child("Waiting for you to approve it on GitHub. The code lasts 14 minutes.");
    parts::screen(cx)
        .child(parts::bar("Sign in with GitHub").start(parts::tap("close", IconName::X)))
        .child(
            v_flex()
                .gap(Space::Medium)
                .p(Space::Medium)
                .child(div().text_color(colors.text_muted).child(
                    "Type this code on GitHub to connect your account. Agents then push and open pull requests as you.",
                ))
                .child(code)
                .child(open)
                .child(waiting)
                .child(cancel),
        )
        .into_any_element()
}
