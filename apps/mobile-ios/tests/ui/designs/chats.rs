//! A workspace's screen, which is its chat: the bar that names it, its pull request's bar, the transcript, and the
//! composer or what the agent asks in its place, with the menus its buttons open.

#![cfg(test)]

use std::rc::Rc;

use client_core::fake_hub::World;
use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, div};
use look::{
    BannerKind, ButtonKind, ControlSize, Corner, IconSize, RowSize, Shaped as _, Space, Type, Typeset as _, Work,
};

use super::canvas::{Canvas, Chat, loaded};
use super::parts::{self, Composer};
use super::worlds;

/// A workspace's screen to draw: what its chat holds, what's under the transcript, and what's over the screen.
struct Shown {
    name: &'static str,
    world: fn() -> World,
    under: Under,
    over: Option<fn(&App) -> AnyElement>,
    /// What's said across the screen under the pull request's bar.
    says: Option<fn(&App) -> AnyElement>,
}

const fn shown(name: &'static str, world: fn() -> World, under: Under) -> Shown {
    Shown { name, world, under, over: None, says: None }
}

const fn over(name: &'static str, over: fn(&App) -> AnyElement) -> Shown {
    Shown { over: Some(over), ..shown(name, worlds::finished, Under::Composer(Composer::Empty, None)) }
}

const SHOWN: &[Shown] = &[
    shown("chat", worlds::finished, Under::Composer(Composer::Empty, None)),
    shown("chat-working", worlds::working, Under::Composer(Composer::Working, None)),
    shown("chat-queued", worlds::working, Under::Queue),
    shown("chat-planning", worlds::finished, Under::Composer(Composer::Planning, Some(PLAN))),
    shown("chat-dictating", worlds::finished, Under::Composer(Composer::Dictating, Some(HEARD))),
    shown("chat-attached", worlds::finished, Under::Composer(Composer::Attached, Some(ATTACHED))),
    shown("chat-question", worlds::asking, Under::Asked),
    shown("chat-tool-use", worlds::asking_to_run, Under::Asked),
    shown("chat-plan", worlds::planned, Under::Asked),
    Shown {
        says: Some(setup_failed),
        ..shown("chat-setup-failed", worlds::finished, Under::Composer(Composer::Empty, None))
    },
    over("chats-menu", super::menus::chats_menu),
    over("add-menu", super::menus::add_menu),
    over("model-menu", super::menus::model_menu),
    over("chat-menu", super::menus::more_menu),
    over("archive-asks", super::menus::archive_asks),
    over("usage-sheet", usage_sheet),
];

pub(super) fn draw(canvas: &mut Canvas) {
    for shown in SHOWN {
        let chat = Rc::new(canvas.chat((shown.world)(), worlds::WORKSPACE));
        let ready = chat.clone();
        canvas.draw_once(
            shown.name,
            move |cx| loaded(&ready, cx),
            move |_, cx| {
                workspace(&chat, shown.under, shown.over.map(|over| over(cx)), shown.says.map(|says| says(cx)), cx)
            },
        );
    }
    let subagent = Rc::new(canvas.chat(worlds::subagent(), worlds::WORKSPACE));
    let ready = subagent.clone();
    canvas.draw_once("subagent", move |cx| loaded(&ready, cx), move |_, cx| self::subagent(&subagent, cx));
}

const PLAN: &str = "Plan how a workspace’s row shows its pull request";
const HEARD: &str = "Keep the badge at its own width and let the name";
const ATTACHED: &str = "The badge covers the name at this width";

/// What's under the transcript.
#[derive(Clone, Copy)]
enum Under {
    Composer(Composer, Option<&'static str>),
    /// The composer, with prompts waiting above it for the agent to finish.
    Queue,
    /// What the agent asks, in the composer's place.
    Asked,
}

fn workspace(chat: &Chat, under: Under, over: Option<AnyElement>, says: Option<AnyElement>, cx: &App) -> AnyElement {
    let bar = parts::bar("Show PR state in the sidebar")
        .under("sidebar-pr-state")
        .start(parts::back())
        .menu("chats", |menu, _, _| menu)
        .end(parts::tap("changes", IconName::FileDiff))
        .end(parts::tap("more", IconName::Ellipsis));
    let foot: AnyElement = match under {
        Under::Composer(stands, typed) => parts::composer(stands, typed, cx).into_any_element(),
        Under::Queue => {
            v_flex().child(queue(cx)).child(parts::composer(Composer::Working, None, cx)).into_any_element()
        }
        Under::Asked => {
            let asked = chat.transcript.read(cx).question().cloned();
            div().w_full().flex_none().px(Space::Medium).py(Space::Small).children(asked).into_any_element()
        }
    };
    parts::screen(cx)
        .child(bar)
        .child(parts::pull_request(Some("#182"), Work::Ready, "Ready to merge", Some("Merge"), cx))
        .children(says)
        .child(parts::chat_room().child(chat.transcript.clone()))
        .child(foot)
        .child(parts::foot())
        .children(over)
        .into_any_element()
}

/// The prompts waiting for the agent to finish, over the composer, as the desktop's queue is.
fn queue(cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let waiting = |id: &'static str, prompt: &'static str| {
        h_flex()
            .gap(Space::Small)
            .p(Space::Small)
            .pl(Space::Medium)
            .corners(Corner::Control)
            .bg(colors.inset)
            .child(div().flex_1().min_w_0().truncate().child(prompt))
            .child(look::icon_button(id, ButtonKind::Quiet, ControlSize::Small, IconName::X))
    };
    let heading = h_flex()
        .px(Space::XSmall)
        .typeset(Type::Detail)
        .text_color(colors.text_muted)
        .child(div().flex_1().child("2 queued, sent when the agent finishes"))
        .child(look::button("send-now", ButtonKind::Quiet, ControlSize::Small, "Send now"));
    v_flex()
        .mx(Space::Medium)
        .p(Space::Small)
        .gap(Space::XSmall)
        .corners_top(Corner::Card)
        .border(look::EDGE)
        .border_color(colors.edge)
        .child(heading)
        .child(waiting("drop-1", "Keep the badge at its own width"))
        .child(waiting("drop-2", "And show it in Home too, in the same colours"))
}

/// What's said across a workspace whose setup script failed, with what to do about it.
fn setup_failed(cx: &App) -> AnyElement {
    let _ = cx;
    look::banner(BannerKind::Warning)
        .child("Setup failed: cargo fetch exited with 101.")
        .answer(look::button("setup-log", ButtonKind::Borderless, ControlSize::Small, "Log"))
        .answer(look::button("setup-again", ButtonKind::Borderless, ControlSize::Small, "Retry"))
        .into_any_element()
}

/// How much of a limit is left, with when it starts again.
fn limit(over: &'static str, left: f32, resets: &'static str, cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    #[expect(clippy::cast_possible_truncation, reason = "a share of a hundred")]
    let said = format!("{}% left", (left * 100.).round() as i32);
    let bar = div()
        .w_full()
        .h(gpui_kit::px(6.))
        .corners(Corner::Glyph)
        .bg(colors.control_edge)
        .child(div().w(gpui_kit::relative(left)).h_full().corners(Corner::Glyph).bg(colors.text));
    v_flex()
        .gap(Space::Small)
        .child(h_flex().child(div().flex_1().child(over)).child(div().emphasis().child(said)))
        .child(bar)
        .child(div().typeset(Type::Detail).text_color(colors.text_muted).child(resets))
}

/// What the composer's ring opens: how full the chat's context is, what's left of its account's limits, and the
/// account it runs under, which it can be moved from.
fn usage_sheet(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let muted = colors.text_muted;
    let context = v_flex()
        .gap(Space::Small)
        .child(h_flex().child(div().flex_1().child("Context")).child(div().emphasis().child("38% used")))
        .child(div().typeset(Type::Detail).text_color(muted).child("76k of 200k tokens · Cached for 4 more minutes"));
    let compact = look::button("compact", ButtonKind::Outlined, ControlSize::Regular, "Compact now").full_width();
    let account = look::list_row("account", RowSize::Large)
        .child(parts::icon(IconName::Bot, IconSize::Large, muted))
        .child(parts::names("ada@example.com", "Max · Claude account", cx))
        .child(div().flex_none().typeset(Type::Detail).text_color(muted).child("Move"))
        .opens();
    look::sheet()
        .title("Claude usage")
        .clear(parts::HOME_INDICATOR)
        .child(
            v_flex()
                .gap(Space::Large)
                .p(Space::Medium)
                .child(context)
                .child(compact)
                .child(look::line())
                .child(limit("5h limit", 0.71, "Resets 6:00 PM", cx))
                .child(limit("7d limit", 0.96, "Resets Sat 9:00 PM", cx)),
        )
        .child(look::line())
        .child(parts::rows().py(look::ROWS_INSET).child(account))
        .into_any_element()
}

/// A subagent's work, opened from its line in the chat: what the agent asked it and what it reported, which nothing
/// is typed into.
fn subagent(chat: &Chat, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let foot = h_flex()
        .flex_none()
        .gap(Space::Small)
        .px(Space::Medium)
        .py(Space::Medium)
        .typeset(Type::Detail)
        .text_color(colors.text_muted)
        .child(parts::icon(IconName::CircleCheck, IconSize::Regular, colors.done))
        .child("Done in 42s · 18.2k tokens");
    parts::screen(cx)
        .child(parts::bar("Find where the sidebar gets records").under("Explore · subagent").start(parts::back()))
        .child(parts::chat_room().child(chat.transcript.clone()))
        .child(div().flex_none().w_full().child(look::line()))
        .child(foot)
        .child(parts::foot())
        .into_any_element()
}
