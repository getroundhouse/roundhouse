//! The hub's workspaces, which the app opens on: the sidebar and Home in one list, a workspace's menu, and starting
//! a workspace.

#![cfg(test)]

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, div};
use look::{
    BadgeKind, ButtonKind, ControlSize, IconSize, RowSize, Signal, Space, SwitchKind, Type, Typeset as _, Work,
};

use super::canvas::Canvas;
use super::parts::{self, Composer};

pub(super) fn draw(canvas: &mut Canvas) {
    canvas.draw("workspaces", |_, cx| workspaces(None, cx));
    canvas.draw("new-workspace", |_, cx| new_workspace(Composer::Empty, cx));
    canvas.draw("new-workspace-planning", |_, cx| new_workspace(Composer::Planning, cx));
}

/// A workspace, as its row shows it.
pub(super) struct Listed {
    id: &'static str,
    title: &'static str,
    /// What its agent is doing, or where its work stands, in a few words.
    stands: &'static str,
    signal: Signal,
    work: Work,
    /// A reply nobody has read, or an agent waiting on an answer.
    needs_a_look: bool,
    /// The lines it has added and removed.
    changed: Option<(usize, usize)>,
    /// What's said at its end in place of the lines, such as that it's being archived.
    says: Option<&'static str>,
    /// Something typed in it isn't sent.
    draft: bool,
}

const fn listed(id: &'static str, title: &'static str, stands: &'static str, signal: Signal, work: Work) -> Listed {
    Listed { id, title, stands, signal, work, needs_a_look: false, changed: None, says: None, draft: false }
}

pub(super) const ROUNDHOUSE: &[Listed] = &[
    Listed {
        needs_a_look: true,
        changed: Some((41, 6)),
        ..listed("cedar", "Ask before deleting files", "Asks which files to keep", Signal::Idle, Work::Draft)
    },
    Listed {
        changed: Some((128, 14)),
        ..listed("aspen", "Show PR state in the sidebar", "#182 · Ready to merge", Signal::Idle, Work::Ready)
    },
    Listed {
        changed: Some((312, 87)),
        draft: true,
        ..listed("birch", "Speed up opening a large diff", "Working · 4m", Signal::Working, Work::NoPr)
    },
    Listed {
        changed: Some((9, 3)),
        ..listed("alder", "Fix the login redirect", "#171 · In the merge queue", Signal::Background, Work::Queued)
    },
    Listed {
        says: Some("Archiving…"),
        ..listed("larch", "Drop the old settings file", "#164 · Merged", Signal::Idle, Work::Merged)
    },
];

const SIGNAL_BOX: &[Listed] = &[
    Listed {
        needs_a_look: true,
        changed: Some((22, 0)),
        ..listed("elm", "Retry a failed setup", "Stopped short · 12m ago", Signal::Failed, Work::NoPr)
    },
    listed("fir", "fir", "Setting up…", Signal::SettingUp, Work::NoPr),
];

pub(super) fn row(listed: &Listed, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    // A name is muted until it needs a look, as in the desktop's sidebar, so the ones that do stand out.
    let name = div().truncate().text_color(if listed.needs_a_look { colors.text } else { colors.text_muted });
    let name = if listed.needs_a_look { name.emphasis() } else { name };
    let end: AnyElement = match (listed.says, listed.changed) {
        (Some(says), _) => div().typeset(Type::Detail).text_color(colors.text_faint).child(says).into_any_element(),
        (None, Some((added, removed))) => look::lines_changed(added, removed).into_any_element(),
        (None, None) => div().into_any_element(),
    };
    let pencil = listed.draft.then(|| parts::icon(IconName::Pencil, IconSize::Regular, colors.text_faint));
    look::list_row(listed.id, RowSize::Large)
        .child(look::mark(listed.signal, listed.work, look::Size::Row, cx))
        .child(parts::names(name.child(listed.title), listed.stands, cx))
        .children(pencil)
        .child(div().flex_none().child(end))
        .into_any_element()
}

fn project(name: &'static str, id: &'static str, workspaces: &[Listed], cx: &App) -> impl IntoElement {
    let add = look::icon_button(id, ButtonKind::Quiet, ControlSize::Small, IconName::Plus);
    v_flex()
        .child(parts::heading(name, Some(add.into_any_element()), cx))
        .children(workspaces.iter().map(|workspace| row(workspace, cx)))
}

/// Which workspaces the list shows, under its bar.
fn narrowing() -> impl IntoElement {
    let choice = |switch: look::Switch, id: &'static str, words: &'static str, chosen| {
        switch.words(id, words, chosen, |_, _| {})
    };
    let switch = look::switch(SwitchKind::Tracked);
    let switch = choice(switch, "all", "All", true);
    let switch = choice(switch, "needs-you", "Needs you", false);
    let switch = choice(switch, "working", "Working", false);
    h_flex().px(Space::Medium).pt(Space::Medium).pb(Space::XSmall).child(switch)
}

pub(super) fn workspaces(over: Option<AnyElement>, cx: &App) -> AnyElement {
    let bar = parts::bar("Ada’s Mac mini")
        .under("Connected")
        .start(parts::tap("settings", IconName::Settings))
        .end(parts::tap("search", IconName::Search))
        .end(parts::tap("new-workspace", IconName::Plus));
    let list = parts::rows()
        .child(project("roundhouse", "new-in-roundhouse", ROUNDHOUSE, cx))
        .child(div().h(Space::Small.size()))
        .child(project("signal-box", "new-in-signal-box", SIGNAL_BOX, cx));
    parts::screen(cx).child(bar).child(narrowing()).child(list).children(over).into_any_element()
}

/// Starting a workspace: where, on which agent, and what to work on. Sending the prompt starts it, as on the desktop.
fn new_workspace(stands: Composer, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let muted = colors.text_muted;
    let chosen = |words: &'static str| {
        h_flex().gap(Space::XSmall).text_color(muted).child(words).child(parts::icon(
            IconName::ChevronsUpDown,
            IconSize::Regular,
            colors.text_faint,
        ))
    };
    let typed = match stands {
        Composer::Planning => Some("Plan how the sidebar shows a workspace’s pull request"),
        Composer::Empty | Composer::Working | Composer::Dictating | Composer::Attached => None,
    };
    let planning = (stands == Composer::Planning)
        .then(|| h_flex().px(Space::Large).pt(Space::Small).child(look::badge(BadgeKind::Plain, "Plan mode")));
    let bar = parts::bar("New workspace").start(parts::tap("close", IconName::X));
    parts::screen(cx)
        .child(bar)
        .child(
            parts::rows()
                .pt(Space::Small)
                .child(parts::setting("project", "Project", chosen("roundhouse")))
                .child(parts::setting("from", "Starts from", chosen("origin/main")))
                .child(parts::setting("agent", "Agent", chosen("Claude"))),
        )
        .children(planning)
        .child(parts::composer(stands, typed, cx))
        .child(div().flex_1())
        .child(parts::foot())
        .into_any_element()
}
