//! Settings: the pages the desktop has, each a screen of its own, under a list that says which are this phone's and
//! which are the hub's, since a setting belongs to whatever carries it out.

#![cfg(test)]

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Hsla, IntoElement, ParentElement as _, Styled as _, div, px};
use look::{
    BadgeKind, BannerKind, ButtonKind, ControlSize, Corner, IconSize, Lined as _, RowSize, Shaped as _, Space, Type,
    Typeset as _,
};

use super::canvas::Canvas;
use super::parts;

pub(super) fn draw(canvas: &mut Canvas) {
    canvas.draw("settings", |_, cx| settings(cx));
    canvas.draw("settings-general", |_, cx| general(cx));
    canvas.draw("settings-computer", |_, cx| computer(cx));
    canvas.draw("settings-notifications", |_, cx| notifications(cx));
    canvas.draw("settings-agents", |_, cx| agents(None, cx));
    canvas.draw("settings-account-menu", |_, cx| agents(Some(super::menus::account_menu(cx)), cx));
}

const HUB: &str = "Ada’s Mac mini";

pub(super) fn page(title: &'static str) -> look::TopBar {
    parts::bar(title).start(parts::back())
}

/// The list of pages: this phone's, then the hub's, by its name.
fn settings(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let about = look::list_row("about", RowSize::Regular)
        .child(parts::icon(IconName::Info, IconSize::Large, colors.text_muted))
        .child(div().flex_1().child("Roundhouse 1.0.0"))
        .child(div().typeset(Type::Detail).text_color(colors.text_muted).child("Up to date"));
    parts::screen(cx)
        .child(parts::bar("Settings").start(parts::tap("close", IconName::X)))
        .child(
            parts::rows()
                .pt(Space::Small)
                .child(parts::heading("This iPhone", None, cx))
                .child(parts::page_row("hub", IconName::Server, "Hub", "Connected", cx))
                .child(parts::page_row("general", IconName::SlidersHorizontal, "General", "", cx))
                .child(parts::page_row("notifications", IconName::Bell, "Notifications", "On", cx))
                .child(div().h(Space::Medium.size()))
                .child(parts::heading(HUB, None, cx))
                .child(parts::page_row("hub-general", IconName::Laptop, "Computer", "Kept awake", cx))
                .child(parts::page_row("agents", IconName::Bot, "Agents", "4 accounts", cx))
                .child(parts::page_row("git", IconName::GitBranch, "Git", "ada-l", cx))
                .child(div().h(Space::Medium.size()))
                .child(look::line())
                .child(about),
        )
        .into_any_element()
}

/// The app's own settings.
fn general(cx: &App) -> AnyElement {
    let chosen = |words: &'static str| {
        let colors = look::colors(cx);
        h_flex().gap(Space::XSmall).text_color(colors.text_muted).child(words).child(parts::icon(
            IconName::ChevronsUpDown,
            IconSize::Regular,
            colors.text_faint,
        ))
    };
    parts::screen(cx)
        .child(page("General"))
        .child(parts::heading("This iPhone", None, cx).px(Space::Medium).pt(Space::Medium))
        .child(parts::described(
            "Always expand thinking",
            "Show what the agent thinks in full, rather than as a line to open.",
            look::toggle("thinking", false),
            cx,
        ))
        .child(parts::described(
            "Dictation",
            "Dictate prompts and answers. Needs a model of 2.4 GB, downloaded once.",
            chosen("Ask first"),
            cx,
        ))
        .child(parts::described("Dictation language", "The language what’s dictated is heard in.", chosen("Auto"), cx))
        .into_any_element()
}

/// The hub's settings that aren't about an account, which hold whichever app shows the hub.
fn computer(cx: &App) -> AnyElement {
    parts::screen(cx)
        .child(page("Computer"))
        .child(
            parts::said("These are the hub’s settings, so every app that shows Ada’s Mac mini shows the same.", cx)
                .pt(Space::Medium),
        )
        .child(look::line())
        .child(parts::described(
            "Keep computer awake while agents are working",
            "Stops the hub’s computer going to sleep by itself while there’s work.",
            look::toggle("awake", true),
            cx,
        ))
        .child(parts::described(
            "Skip Claude’s safety checks",
            "Answers Allow for you when an agent with full access asks before using a tool anyway.",
            look::toggle("skip", false),
            cx,
        ))
        .into_any_element()
}

fn notifications(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let sound = h_flex().gap(Space::XSmall).text_color(colors.text_muted).child("Chime").child(parts::icon(
        IconName::ChevronsUpDown,
        IconSize::Regular,
        colors.text_faint,
    ));
    let asked = look::banner(BannerKind::Note)
        .child("Notifications are off for Roundhouse in this iPhone’s Settings.")
        .answer(look::button("open-settings", ButtonKind::Borderless, ControlSize::Small, "Open Settings"));
    parts::screen(cx)
        .child(page("Notifications"))
        .child(asked)
        .child(parts::described(
            "Notify when an agent needs you",
            "When a chat has a reply nobody has read, waits on an answer or an approval, or stops short. Never for the \
             chat you’re looking at.",
            look::toggle("notify", true),
            cx,
        ))
        .child(parts::described(
            "Play a sound",
            "At the same moments, whether or not a notification shows.",
            look::toggle("sound", true),
            cx,
        ))
        .child(parts::described("Sound", "Choosing one plays it.", sound, cx))
        .into_any_element()
}

/// How much of a limit is left, as a bar after what the limit is.
fn limit(over: &'static str, left: f32, cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let (whole, tall) = (px(36.), px(4.));
    let bar = div()
        .w(whole)
        .h(tall)
        .corners(Corner::Glyph)
        .bg(colors.control_edge)
        .child(div().w(whole * left).h(tall).corners(Corner::Glyph).bg(colors.text));
    #[expect(clippy::cast_possible_truncation, reason = "a share of a hundred")]
    let said = format!("{}%", (left * 100.).round() as i32);
    h_flex()
        .gap(Space::XSmall)
        .typeset(Type::Detail)
        .text_color(colors.text_muted)
        .child(over)
        .child(bar)
        .child(div().text_color(colors.text).child(said))
}

/// The robot in a square every account's row starts with.
fn robot(cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    div()
        .flex_none()
        .size(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .corners(Corner::Control)
        .bg(colors.raised)
        .child(parts::icon(IconName::Bot, IconSize::Large, colors.text_muted))
}

/// What an account's row says after its name.
enum Stands {
    Limits(f32, f32),
    Says(&'static str, Option<Hsla>),
}

/// An account agents run under: its mark, its name over how its login was made, then how it stands, and its menu.
fn account(
    id: &'static str,
    name: &'static str,
    default: bool,
    made: &'static str,
    stands: &Stands,
    menu: bool,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let gone = matches!(stands, Stands::Says("Disconnected", _));
    let named = h_flex()
        .gap(Space::Small)
        .child(div().min_w_0().truncate().emphasis().when(gone, |name| name.text_color(colors.text_muted)).child(name))
        .children(default.then(|| look::badge(BadgeKind::Plain, "Default")));
    let stands = match stands {
        Stands::Limits(five_hours, week) => h_flex()
            .gap(Space::Medium)
            .child(limit("5h", *five_hours, cx))
            .child(limit("7d", *week, cx))
            .into_any_element(),
        Stands::Says(says, tone) => {
            let tone = tone.unwrap_or(colors.text_muted);
            h_flex()
                .gap(Space::XSmall)
                .typeset(Type::Detail)
                .text_color(tone)
                .children(
                    (tone != colors.text_muted).then(|| parts::icon(IconName::TriangleAlert, IconSize::Small, tone)),
                )
                .child(*says)
                .into_any_element()
        }
    };
    let about = v_flex()
        .flex_1()
        .min_w_0()
        .gap(Space::XSmall)
        .child(named)
        .child(div().typeset(Type::Detail).text_color(colors.text_muted).truncate().child(made))
        .child(stands);
    h_flex()
        .w_full()
        .gap(Space::Medium)
        .px(Space::Medium)
        .py(Space::Medium)
        .line_under(cx)
        .child(robot(cx))
        .child(about)
        .children(menu.then(|| look::icon_button(id, ButtonKind::Borderless, ControlSize::Large, IconName::Ellipsis)))
        .into_any_element()
}

fn agents(over: Option<AnyElement>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let chosen = h_flex().gap(Space::XSmall).text_color(colors.text_muted).child("Work").child(parts::icon(
        IconName::ChevronsUpDown,
        IconSize::Regular,
        colors.text_faint,
    ));
    let connect =
        look::button("connect", ButtonKind::Outlined, ControlSize::Small, "Connect account").icon(IconName::Plus);
    parts::screen(cx)
        .child(page("Agents"))
        .child(parts::heading("Default accounts", None, cx).px(Space::Medium).pt(Space::Medium))
        .child(parts::said(
            "Which account new chats use across this hub. A chat stays on its account until you move it.",
            cx,
        ))
        .child(parts::described("Claude Code", "Chats already on another account stay there.", chosen, cx))
        .child(parts::heading("Found", None, cx).px(Space::Medium).pt(Space::Large))
        .child(parts::said("The logins Claude Code already had where agents ran.", cx))
        .child(account(
            "found-1",
            "ada@example.com",
            false,
            "Max · Claude account",
            &Stands::Limits(0.71, 0.96),
            false,
            cx,
        ))
        .child(parts::heading("Added", Some(connect.into_any_element()), cx).px(Space::Medium).pt(Space::Large))
        .child(parts::said("Logins kept by this hub.", cx))
        .child(account(
            "added-1",
            "Personal",
            false,
            "Claude Code",
            &Stands::Says("Ends in 9 days, on Nov 10, 2026", Some(colors.warning)),
            true,
            cx,
        ))
        .child(account("added-2", "Work", true, "Claude Code", &Stands::Limits(0.38, 0.82), true, cx))
        .child(account("added-3", "Contractor", false, "Claude Code", &Stands::Says("Disconnected", None), true, cx))
        .children(over)
        .into_any_element()
}
