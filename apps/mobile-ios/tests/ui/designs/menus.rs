//! Menus as an iPhone's own apps show them, as `components.md` says a handheld's are: one that drops from the button
//! that opened it, under it or over it near the screen's bottom, and one that a long press on a row opens, with the
//! row lifted over the dimmed screen.

#![cfg(test)]

use gpui_kit::assets::IconName;
use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Pixels, Styled as _, div, px};
use look::{CountKind, IconSize, RowSize, Signal, Space, Type, Typeset as _};

use super::canvas::Canvas;
use super::lists;
use super::parts;

pub(super) fn draw(canvas: &mut Canvas) {
    canvas.draw("workspace-context-menu", |_, cx| lists::workspaces(Some(context_menu(cx)), cx));
}

/// How wide a menu is: a little over half the screen, as an iPhone's are.
const WIDTH: Pixels = px(250.);
/// How far in from the screen's edge a menu sits.
const IN: Pixels = px(12.);

/// A row of a menu: what it does, with its icon at its end, as an iPhone's menus put it.
fn item(id: &'static str, glyph: IconName, words: &'static str, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    look::list_row(id, RowSize::Regular)
        .child(div().flex_1().min_w_0().truncate().child(words))
        .child(parts::icon(glyph, IconSize::Large, colors.text_muted))
        .into_any_element()
}

fn archive(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    look::list_row("archive", RowSize::Regular)
        .child(div().flex_1().text_color(colors.error).child("Archive"))
        .child(parts::icon(IconName::Archive, IconSize::Large, colors.error))
        .into_any_element()
}

/// A menu's surface: raised, with a card's corners and the shadow, holding its rows.
fn surface(width: Pixels, rows: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    look::raised(cx).w(width).py(look::ROWS_INSET).child(v_flex().px(look::ROWS_INSET).children(rows))
}

/// What the chat's `…` drops down: what else is done with the workspace, under the button, over the chat.
pub(super) fn more_menu(cx: &App) -> AnyElement {
    let rows = vec![
        item("find", IconName::Search, "Find in chat", cx),
        item("files", IconName::Files, "All files", cx),
        item("setup", IconName::ScanLine, "Setup log", cx),
        item("copy-branch", IconName::GitBranch, "Copy branch name", cx),
        item("rename", IconName::Pencil, "Rename…", cx),
        look::line().into_any_element(),
        archive(cx),
    ];
    placed(Place::Under { right: IN }, WIDTH, rows, cx)
}

/// Where the long-pressed row is on the workspaces' screen, from its top.
const PRESSED_ROW: Pixels = px(256.);

/// What a long press on a workspace's row opens: the row, lifted over the dimmed screen, and its menu under it.
fn context_menu(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let lifted = look::raised(cx).w_full().px(look::ROWS_INSET).child(lists::row(&lists::ROUNDHOUSE[1], cx));
    let rows = vec![
        item("mark-unread", IconName::CircleDot, "Mark as unread", cx),
        item("rename", IconName::Pencil, "Rename…", cx),
        item("regenerate", IconName::Sparkles, "Regenerate title", cx),
        item("open-pr", IconName::ArrowUpRight, "Open #182 on GitHub", cx),
        look::line().into_any_element(),
        archive(cx),
    ];
    div()
        .absolute()
        .inset_0()
        .bg(colors.dim)
        .child(
            v_flex()
                .absolute()
                .top(PRESSED_ROW)
                .left(IN)
                .right(IN)
                .gap(Space::Small)
                .child(lifted)
                .child(surface(WIDTH, rows, cx)),
        )
        .into_any_element()
}

/// Where a menu sits on the screen, by what opened it.
#[derive(Clone, Copy)]
enum Place {
    /// Under a button in the bar across the top, its right edge `right` in from the screen's.
    Under { right: Pixels },
    /// Under the bar's title, in the middle.
    UnderTheTitle,
    /// Over a button near the screen's bottom, its bottom edge `bottom` up from the screen's, and `left` in.
    Over { bottom: Pixels, left: Pixels },
    /// Over a button at a row's end, `bottom` up from the screen's bottom.
    OverTheEnd { bottom: Pixels },
}

/// The bar across the top's bottom edge, which a menu from one of its buttons hangs under.
fn under_the_bar() -> Pixels {
    parts::STATUS_BAR + look::BarSize::Regular.height() + Space::XSmall.size()
}

fn placed(place: Place, width: Pixels, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    let at = div().absolute();
    let at = match place {
        Place::Under { right } => at.top(under_the_bar()).right(right),
        Place::UnderTheTitle => at.top(under_the_bar()).left_0().right_0().flex().justify_center(),
        Place::Over { bottom, left } => at.bottom(bottom).left(left),
        Place::OverTheEnd { bottom } => at.bottom(bottom).right(IN),
    };
    at.child(surface(width, rows, cx)).into_any_element()
}

/// A row of something to choose, with a note after its words, and a check at its end while it's the one chosen.
fn choice(id: &'static str, words: &'static str, note: Option<&'static str>, chosen: bool, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let note = note.map(|note| div().typeset(Type::Detail).text_color(colors.text_muted).child(note));
    look::list_row(id, RowSize::Regular)
        .child(div().flex_none().child(words))
        .child(div().flex_1().min_w_0().truncate().children(note))
        .children(chosen.then(|| parts::icon(IconName::Check, IconSize::Large, colors.text)))
        .into_any_element()
}

/// A label over the rows that follow, which choose one of them.
fn label(words: &'static str, cx: &App) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    div()
        .h(RowSize::Small.height())
        .flex()
        .items_center()
        .px(Space::Small)
        .typeset(Type::Detail)
        .text_color(muted)
        .child(words)
        .into_any_element()
}

/// The space between a composer's button and the menu over it: the home indicator, the composer and a little room.
const OVER_THE_COMPOSER: Pixels = px(180.);

/// What the chat's title opens: the workspace's chats and terminals, and starting another.
pub(super) fn chats_menu(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let chat = |id: &'static str, signal: Signal, title: &'static str, unread: bool, showing: bool| {
        let name = div().flex_1().min_w_0().truncate();
        let name = if unread { name.emphasis() } else { name };
        look::list_row(id, RowSize::Regular)
            .child(look::chat_mark(signal, look::Size::Inline, cx))
            .child(name.child(title))
            .children(unread.then(|| look::count(CountKind::NeedsALook, 1)))
            .children(showing.then(|| parts::icon(IconName::Check, IconSize::Large, colors.text)))
            .into_any_element()
    };
    let rows = vec![
        chat("chat-1", Signal::Idle, "The sidebar should show each workspace’s PR state", false, true),
        chat("chat-2", Signal::Working, "Which colours does GitHub use for a PR’s state?", false, false),
        chat("chat-3", Signal::Idle, "Why does the hub log a warning on every reconnect?", true, false),
        item("terminal-1", IconName::SquareTerminal, "zsh", cx),
        look::line().into_any_element(),
        item("new-chat", IconName::MessageSquarePlus, "New chat", cx),
        item("new-terminal", IconName::Terminal, "New terminal", cx),
        look::line().into_any_element(),
        item("closed", IconName::ChevronRight, "Closed chats", cx),
    ];
    placed(Place::UnderTheTitle, px(330.), rows, cx)
}

/// What the composer's `+` opens: what to add to the prompt, and plan mode, checked while it's on.
pub(super) fn add_menu(cx: &App) -> AnyElement {
    let rows = vec![
        item("photos", IconName::Image, "Photo library", cx),
        item("camera", IconName::Camera, "Take a photo", cx),
        item("files", IconName::Paperclip, "Choose files", cx),
        look::line().into_any_element(),
        choice("plan", "Plan mode", None, false, cx),
    ];
    placed(Place::Over { bottom: OVER_THE_COMPOSER, left: IN + Space::Small.size() }, WIDTH, rows, cx)
}

/// What the composer's model opens: the model, then its effort, each checked where it's chosen.
pub(super) fn model_menu(cx: &App) -> AnyElement {
    let rows = vec![
        choice("fable", "Fable", Some("Most capable"), false, cx),
        choice("opus", "Opus", None, true, cx),
        choice("sonnet", "Sonnet", None, false, cx),
        choice("haiku", "Haiku", Some("Fastest"), false, cx),
        look::line().into_any_element(),
        label("Effort", cx),
        choice("low", "Low", None, false, cx),
        choice("medium", "Medium", None, false, cx),
        choice("high", "High", None, false, cx),
        choice("xhigh", "Extra high", None, true, cx),
    ];
    placed(Place::Over { bottom: OVER_THE_COMPOSER, left: px(60.) }, WIDTH, rows, cx)
}

/// What an account's `…` opens: over the button, from just above it, since there's no room for it under.
pub(super) fn account_menu(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let disconnect = look::list_row("disconnect", RowSize::Regular)
        .child(div().flex_1().text_color(colors.error).child("Disconnect"))
        .child(parts::icon(IconName::CircleX, IconSize::Large, colors.error))
        .into_any_element();
    let rows = vec![
        item("make-default", IconName::CircleCheck, "Make default", cx),
        item("renew", IconName::RefreshCw, "Sign in again", cx),
        item("rename", IconName::Pencil, "Rename…", cx),
        look::line().into_any_element(),
        disconnect,
    ];
    placed(Place::OverTheEnd { bottom: px(276.) }, WIDTH, rows, cx)
}

/// What archiving from a handheld's menu asks first, as an iPhone's action sheet: along the screen's bottom, over
/// the dimmed screen, what's asked over the choice that removes, in red, and Cancel apart under them.
pub(super) fn archive_asks(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let answer = || div().h(RowSize::Large.height()).flex().items_center().justify_center();
    let asked = v_flex()
        .child(
            div()
                .py(Space::Medium)
                .text_center()
                .typeset(Type::Detail)
                .text_color(colors.text_muted)
                .child("Archive Show PR state in the sidebar?"),
        )
        .child(look::line())
        .child(answer().text_color(colors.error).child("Archive"));
    let cancel = answer().emphasis().child("Cancel");
    div()
        .absolute()
        .inset_0()
        .bg(colors.dim)
        .child(
            v_flex()
                .absolute()
                .bottom(parts::HOME_INDICATOR)
                .left(IN)
                .right(IN)
                .gap(Space::Small)
                .child(look::raised(cx).child(asked))
                .child(look::raised(cx).child(cancel)),
        )
        .into_any_element()
}
