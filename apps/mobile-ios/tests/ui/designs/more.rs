//! The screens a workspace opens beside its chat, and the search across the hub: a proposal to answer, a terminal,
//! and finding a workspace or a chat by what it's called.

#![cfg(test)]

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Hsla, IntoElement, ParentElement as _, Styled as _, div, px};
use look::{
    BadgeKind, ButtonKind, CardKind, CardWidth, CodeLines, ControlSize, Corner, IconSize, Lined as _, RowSize,
    Shaped as _, Signal, Space, Type, Typeset as _, Work,
};

use super::canvas::Canvas;
use super::parts;

pub(super) fn draw(canvas: &mut Canvas) {
    canvas.draw("proposal", |_, cx| proposal(cx));
    canvas.draw("terminal", |_, cx| terminal(cx));
    canvas.draw("search", |_, cx| search(cx));
}

/// One of a proposal's options: its letter, what it's called, whether the agent recommends it, what it is, and the
/// button that chooses it.
fn option(
    letter: &'static str,
    label: &'static str,
    recommended: bool,
    says: &'static str,
    cx: &App,
) -> impl IntoElement {
    let colors = look::colors(cx);
    let named = h_flex()
        .gap(Space::Small)
        .child(look::badge(BadgeKind::Plain, letter))
        .child(div().flex_1().min_w_0().truncate().emphasis().child(label))
        .children(recommended.then(|| look::badge(BadgeKind::Plain, "Recommended")));
    let choose =
        look::button(letter, ButtonKind::Outlined, ControlSize::Regular, format!("Choose {letter}")).full_width();
    look::card(CardKind::InPlace, CardWidth::Fills)
        .child(named)
        .child(div().text_color(colors.text_muted).child(says))
        .child(choose)
}

/// What compares a question's options, as a table with a column for each.
fn comparison(cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let row = |cells: [&'static str; 4], heads: bool| {
        let [about, a, b, c] = cells;
        let cell = |words: &'static str| div().flex_1().min_w_0().px(Space::Small).py(Space::Small).child(words);
        h_flex()
            .items_start()
            .typeset(Type::Detail)
            .when(heads, |row| row.text_color(colors.text_muted).bg(colors.inset))
            .line_under(cx)
            .child(cell(about).text_color(colors.text_muted))
            .child(cell(a))
            .child(cell(b))
            .child(cell(c))
    };
    v_flex()
        .flex_none()
        .corners(Corner::Control)
        .border(look::EDGE)
        .border_color(colors.edge)
        .overflow_hidden()
        .child(row(["", "A", "B", "C"], true))
        .child(row(["Survives a restart", "Yes", "No", "Yes"], false))
        .child(row(["New state", "One field", "None", "One table"], false))
        .child(row(["Work", "A day", "Half a day", "Three days"], false))
}

/// A proposal, which a tap on its line in the chat opens: what it's about, what it asks, and its options to choose
/// from, over a bar that answers it.
fn proposal(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let page = v_flex()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .gap(Space::Medium)
        .p(Space::Medium)
        .child(v_flex().flex_none().gap(Space::XSmall).child(div().typeset(Type::Heading).child("Context")).child(
            div().text_color(colors.text_muted).child(
                "A train held at a signal is let go after a minute, whatever the signal says. Dispatch wants to \
                     hold one for longer.",
            ),
        ))
        .child(
            v_flex()
                .flex_none()
                .gap(Space::XSmall)
                .child(div().typeset(Type::Detail).text_color(colors.text_muted).child("Holding"))
                .child(div().emphasis().child("Where does a held train’s wait live?")),
        )
        .child(comparison(cx))
        .child(option("A", "The signal", true, "Each signal keeps how long it’s held its train.", cx))
        .child(option("B", "The train", false, "Each train counts its own wait, and forgets it on a restart.", cx));
    let answer = v_flex()
        .gap(Space::Small)
        .p(Space::Small)
        .corners(Corner::Card)
        .border(look::EDGE)
        .border_color(colors.control_edge)
        .bg(colors.inset)
        .child(
            div()
                .min_h(px(36.))
                .px(Space::Small)
                .pt(Space::XSmall)
                .text_color(colors.text_faint)
                .child("Add a note to your answer"),
        )
        .child(
            h_flex()
                .gap(Space::Small)
                .child(div().flex_1().px(Space::Small).text_color(colors.text_faint).child("Not chosen"))
                .child(
                    look::button("another", ButtonKind::Outlined, ControlSize::Regular, "Another round")
                        .icon(IconName::RefreshCw),
                )
                .child(look::button("send", ButtonKind::Main, ControlSize::Regular, "Send").off(true)),
        );
    parts::screen(cx)
        .child(parts::bar("Holding a train").under("Proposal · waiting for you").start(parts::back()))
        .child(page)
        .child(div().flex_none().px(Space::Medium).py(Space::Small).line_over(cx).child(answer))
        .child(parts::foot())
        .into_any_element()
}

/// The keys a phone's keyboard lacks, in a row under a terminal.
fn terminal_keys(cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let key = |id: &'static str, words: &'static str| look::button(id, ButtonKind::Edged, ControlSize::Regular, words);
    let arrow =
        |id: &'static str, glyph: IconName| look::icon_button(id, ButtonKind::Edged, ControlSize::Regular, glyph);
    h_flex()
        .flex_none()
        .gap(Space::XSmall)
        .px(Space::Small)
        .py(Space::Small)
        .bg(colors.raised)
        .line_over(cx)
        .child(key("esc", "esc"))
        .child(key("tab", "tab"))
        .child(key("ctrl", "ctrl"))
        .child(div().flex_1())
        .child(arrow("left", IconName::ArrowLeft))
        .child(arrow("up", IconName::ArrowUp))
        .child(arrow("down", IconName::ArrowDown))
        .child(arrow("right", IconName::ArrowRight))
}

/// A terminal in the workspace's checkout, with the keys a phone's keyboard lacks in a row under it.
fn terminal(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let line = |words: &'static str, tone: Hsla| {
        div().flex_none().h(CodeLines::File.height()).whitespace_nowrap().text_color(tone).child(words)
    };
    let passed = |test: &'static str| {
        h_flex()
            .flex_none()
            .h(CodeLines::File.height())
            .gap(Space::Small)
            .child(div().whitespace_nowrap().child(test))
            .child(div().text_color(colors.added).child("ok"))
    };
    let cursor = div().w(px(9.)).h(px(17.)).bg(colors.text);
    let grid = v_flex()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .p(Space::Medium)
        .typeset_code(cx)
        .child(line("aspen % cargo test -p hub pull_requests", colors.text))
        .child(line("   Compiling hub v0.1.0 (/workspaces/aspen)", colors.text_muted))
        .child(line("    Finished `test` profile in 14.21s", colors.text_muted))
        .child(line("     Running unittests src/lib.rs", colors.text_muted))
        .child(line("", colors.text))
        .child(line("running 4 tests", colors.text))
        .child(passed("test asks_github_once_a_branch ..."))
        .child(passed("test keeps_what_it_found ..."))
        .child(passed("test tells_every_client ..."))
        .child(passed("test forgets_a_closed_one ..."))
        .child(line("", colors.text))
        .child(line("test result: ok. 4 passed; 0 failed", colors.text))
        .child(line("", colors.text))
        .child(h_flex().h(CodeLines::File.height()).gap(Space::Small).child("aspen %").child(cursor));
    parts::screen(cx)
        .child(
            parts::bar("zsh")
                .under("sidebar-pr-state")
                .start(parts::back())
                .menu("chats", |menu, _, _| menu)
                .end(parts::tap("more", IconName::Ellipsis)),
        )
        .child(grid)
        .child(terminal_keys(cx))
        .child(parts::foot().bg(colors.raised))
        .into_any_element()
}

/// What's searched for, across the screen's top, with the button that leaves the search.
fn search_field(cx: &App) -> impl IntoElement {
    h_flex()
        .flex_none()
        .gap(Space::Small)
        .pt(parts::STATUS_BAR + Space::Small.size())
        .pb(Space::Small)
        .pl(Space::Medium)
        .pr(Space::XSmall)
        .line_under(cx)
        .child(div().flex_1().child(parts::field(Some(IconName::Search), Some("badge"), "Search", cx)))
        .child(look::button("cancel", ButtonKind::Borderless, ControlSize::Regular, "Cancel"))
}

/// Finding a workspace or a chat by what it's called, across the hub.
fn search(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let workspace = |id: &'static str, title: &'static str, under: &'static str, signal: Signal, work: Work| {
        look::list_row(id, RowSize::Large)
            .child(look::mark(signal, work, look::Size::Row, cx))
            .child(parts::names(title, under, cx))
    };
    let chat = |id: &'static str, title: &'static str, under: &'static str| {
        look::list_row(id, RowSize::Large)
            .child(div().w(look::Size::Row.width()).flex().justify_center().child(look::chat_mark(
                Signal::Idle,
                look::Size::Inline,
                cx,
            )))
            .child(parts::names(title, under, cx))
    };
    let archived = look::list_row("archived", RowSize::Large)
        .child(div().w(look::Size::Row.width()).flex().justify_center().child(parts::icon(
            IconName::Archive,
            IconSize::Regular,
            colors.text_faint,
        )))
        .child(parts::names(
            div().text_color(colors.text_muted).child("Give the badge a tooltip"),
            "roundhouse · Archived Nov 3",
            cx,
        ));
    parts::screen(cx)
        .child(search_field(cx))
        .child(
            parts::rows()
                .pt(Space::Small)
                .child(parts::heading("Workspaces", None, cx))
                .child(workspace(
                    "found-1",
                    "Keep the badge at its own width",
                    "roundhouse · Working · 2m",
                    Signal::Working,
                    Work::NoPr,
                ))
                .child(workspace(
                    "found-2",
                    "Show PR state in the sidebar",
                    "roundhouse · #182 · Ready to merge",
                    Signal::Idle,
                    Work::Ready,
                ))
                .child(archived)
                .child(div().h(Space::Small.size()))
                .child(parts::heading("Chats", None, cx))
                .child(chat("chat-1", "Which colours does a badge use for each state?", "Show PR state in the sidebar"))
                .child(chat("chat-2", "Why is the badge cut short in Home?", "Fix the login redirect")),
        )
        .into_any_element()
}
