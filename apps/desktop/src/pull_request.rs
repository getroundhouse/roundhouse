//! A workspace's pull request as the app shows it: a bar at the top of the sidebar of changes, in the pull request's
//! colour, saying the first thing in the way of merging it and offering the one thing to do about it. While the
//! sidebar is hidden, the header shows its number and that action instead.

use std::time::Duration;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled as _, Window,
    div,
};
use look::Work;
use protocol::model::{PullRequest, Standing, WorkspaceId};

use crate::busy::Busy;

#[cfg(test)]
mod tests;
mod words;

pub use words::{Action, Ask};

/// How often the pull request showing is read again. Soon enough to see a check finish or the queue merge it within
/// a minute, and far from GitHub's limits, at two calls to it each time.
pub const READ_AGAIN: Duration = Duration::from_secs(30);

/// As tall as the workspace's header, so the bar lines up with it across the window, and the window moves by it too.
const HEIGHT: Pixels = crate::workspace_pane::HEADER_HEIGHT;

/// What the app knows of a workspace's pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// Not asked yet. Saying so, rather than offering to create one, keeps a workspace that has one from offering it.
    Looking,
    Found(Option<PullRequest>),
}

impl Lookup {
    pub fn pull_request(&self) -> Option<&PullRequest> {
        match self {
            Lookup::Looking => None,
            Lookup::Found(pr) => pr.as_ref(),
        }
    }

    /// `None` until it's known.
    pub fn standing(&self) -> Option<Standing> {
        match self {
            Lookup::Looking => None,
            Lookup::Found(pr) => Some(Standing::of(pr.as_ref())),
        }
    }

    pub fn work(&self) -> Work {
        self.standing().map_or(Work::NoPr, words::work)
    }

    /// Takes the hub's answer, unless a newer ask has been made, whose answer can arrive first, so this one says
    /// what's no longer true. A failure keeps what was known, since it only means it can't be told now, such as while
    /// the workspace sets up. Says whether anything changed.
    pub(crate) fn answer<E>(&mut self, newest: bool, found: Result<Option<PullRequest>, E>) -> bool {
        match found {
            Ok(found) if newest => {
                *self = Lookup::Found(found);
                true
            }
            Ok(_) | Err(_) => false,
        }
    }
}

/// The bar at the top of the sidebar: the pull request's number, which opens it, what stands in its way, and the
/// one thing to do about it, all in its colour. `busy` greys out Archive, as every button that archives is while a
/// workspace is being made, archived or restored, and it says Archiving… while `workspace`, the one this is for, is.
pub fn bar(
    lookup: &Lookup,
    busy: &Busy,
    workspace: &WorkspaceId,
    on_action: impl Fn(&Action, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let work = lookup.work();
    let theme = cx.theme();
    let row = h_flex()
        .id("pull-request-bar")
        .flex_none()
        .w_full()
        .h(HEIGHT)
        .px_3()
        .gap_2()
        .bg(work.tint(cx))
        .border_b_1()
        .border_color(theme.border);
    let Some(standing) = lookup.standing() else {
        return row
            .child(div().text_sm().text_color(look::faint()).child("Looking for its pull request…"))
            .into_any_element();
    };
    let pr = lookup.pull_request();
    let title = words::title(standing, pr.and_then(|pr| pr.base.as_deref()));
    let text = if pr.is_some() { work.text(cx) } else { theme.muted_foreground };
    row.children(pr.map(|pr| number(pr, work, cx)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(text)
                .child(title),
        )
        .children(words::action(standing).map(|action| action_button(action, work, (busy, workspace), on_action, cx)))
        .into_any_element()
}

/// The same, for the header while the sidebar is hidden: the number and the action, without the words, which the
/// sidebar has room for and the header doesn't.
pub fn controls(
    lookup: &Lookup,
    busy: &Busy,
    workspace: &WorkspaceId,
    on_action: impl Fn(&Action, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    let standing = lookup.standing()?;
    let work = lookup.work();
    let pr = lookup.pull_request();
    let controls =
        h_flex().id("pull-request-controls").gap_2().children(pr.map(|pr| number(pr, work, cx))).children(
            words::action(standing).map(|action| action_button(action, work, (busy, workspace), on_action, cx)),
        );
    Some(controls.into_any_element())
}

/// The pull request's number, outlined as a button that opens something is, which opens it on GitHub.
fn number(pr: &PullRequest, work: Work, cx: &App) -> AnyElement {
    let url = pr.url.clone();
    Button::new("pull-request")
        .small()
        .outline()
        .border_color(work.edge(cx))
        .text_color(work.text(cx))
        .tooltip(pr.title.clone())
        .label(format!("#{}", pr.number))
        .child(Icon::new(Lucide::ArrowUpRight).xsmall())
        .on_click(move |_, _, cx| cx.open_url(&url))
        .into_any_element()
}

/// The main action, filled with the pull request's colour, with the theme's dark text on it as every main action
/// has. One with no colour of its own is the theme's main button.
fn action_button(
    action: Action,
    work: Work,
    (busy, workspace): (&Busy, &WorkspaceId),
    on_action: impl Fn(&Action, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    // Greyed out while a workspace is being made, archived or restored, as every button that archives is, so it can't
    // be pressed twice. The other actions only ask the agent.
    let disabled = action == Action::Archive && !busy.is_free();
    let label = busy.doing(workspace).filter(|_| action == Action::Archive).unwrap_or(action.label());
    let button = Button::new("pull-request-action").small().label(label).disabled(disabled);
    let button = match work.hue() {
        // The kit draws a custom button's colour faintly, so the fill is set on the button itself, which hover and
        // press still change. Not while it's disabled, since the kit lays the button's own style over its disabled
        // one, which would keep the fill.
        Some(fill) => {
            let style = ButtonCustomVariant::new(cx)
                .color(fill)
                .foreground(cx.theme().primary_foreground)
                .hover(fill.opacity(0.85))
                .active(fill.opacity(0.7));
            button.custom(style).when(!disabled, |button| button.bg(fill))
        }
        None => button.primary(),
    };
    button.on_click(move |_, window, cx| on_action(&action, window, cx)).into_any_element()
}
