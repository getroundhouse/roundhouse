//! A workspace's pull request as the app shows it: a bar at the top of the sidebar of changes, in the pull request's
//! colour, saying the first thing in the way of merging it and offering the one thing to do about it. While the
//! sidebar is hidden, the header shows its number and that action instead.

use std::time::Duration;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled as _,
    Window, div, px,
};
use look::Work;
use look::{Hand as _, Off as _};
use protocol::model::{BranchName, Local, PullRequest, Standing, Workspace};
use transcript::PullRequestStanding;

use crate::busy::Busy;
use crate::sides::{self, AtTheTop};

#[cfg(test)]
mod tests;
mod words;

pub use words::{Action, Ask};

/// How often the pull request showing is read again. Soon enough to see a check finish or the queue merge it within
/// a minute, and far from GitHub's limits, at two calls to it each time. The hub reads every workspace's less often.
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
    /// What the hub last found, as the workspace's record has it. `None` until the hub has looked, and from a hub too
    /// old to keep it there.
    pub fn recorded(workspace: &Workspace) -> Option<Self> {
        workspace.pull_request.clone().map(|found| Lookup::Found(found.pull_request))
    }

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

    /// Where it stands on GitHub, which a chat's line for a wait on it says, as [`words::for_a_wait`] does, leaving out
    /// the worktree, since a wait goes by what GitHub says. `None` until it's known, with none, and where that says
    /// nothing.
    pub fn for_a_wait(&self) -> Option<PullRequestStanding> {
        let pr = self.pull_request()?;
        let on_github = PullRequest { local: Local::default(), ..pr.clone() };
        let words =
            words::for_a_wait(Standing::of(Some(&on_github)), pr.checks, pr.base.as_ref().map(BranchName::as_str))?;
        Some(PullRequestStanding { number: pr.number, words })
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

/// Where a workspace's work stands, as its record says, for its mark. `None` where [`Lookup::recorded`] is.
pub(crate) fn recorded_work(workspace: &Workspace) -> Option<Work> {
    workspace.pull_request.as_ref().map(|found| words::work(Standing::of(found.pull_request.as_ref())))
}

/// The bar at the top of the sidebar: the pull request's number, which opens it, what stands in its way, and the
/// one thing to do about it, all in its colour. `busy` greys out Archive, as every button that archives is while the
/// app is making, archiving or restoring a workspace, and it says what the workspace it's for is `doing` in its place,
/// such as `Archiving…`, as [`crate::busy::doing`] reads it from its record.
pub fn bar(
    lookup: &Lookup,
    busy: &Busy,
    doing: Option<&'static str>,
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
        .pl_3()
        .pr(px(12.) + sides::clearance(AtTheTop::RightSidebar, cx).end)
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
    let title = words::title(standing, pr.and_then(|pr| pr.base.as_ref()).map(BranchName::as_str));
    let text = if pr.is_some() { work.text(cx) } else { theme.muted_foreground };
    let title = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(text)
        .child(title);
    row.child(giving_way(pr.map(|pr| number(pr, work, cx)), title))
        .children(words::action(standing).map(|action| action_button(action, work, (busy, doing), on_action, cx)))
        .into_any_element()
}

/// The number, then the title, filling what the action leaves of the bar. As the bar narrows, the title gives way
/// first, as far as nothing, and then the number goes, rather than push the action past the bar's end, which on
/// Windows is under the window's buttons. They're laid out from the right and wrap, so the number is the one to go: it
/// wraps onto a second line, which the bar's height leaves out of sight, since each fills that height. An action
/// longer than the whole bar still runs past it.
fn giving_way(number: Option<AnyElement>, title: Div) -> impl IntoElement {
    let filling = |child: AnyElement| div().h_full().flex().items_center().child(child);
    let title = filling(title.into_any_element()).flex_1().min_w_0();
    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_row_reverse()
        .flex_wrap()
        .overflow_hidden()
        .gap_2()
        .child(title)
        .children(number.map(filling))
}

/// The same, for the header while the sidebar is hidden: the number and the action, without the words, which the
/// sidebar has room for and the header doesn't.
pub fn controls(
    lookup: &Lookup,
    busy: &Busy,
    doing: Option<&'static str>,
    on_action: impl Fn(&Action, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    let standing = lookup.standing()?;
    let work = lookup.work();
    let pr = lookup.pull_request();
    let controls = h_flex()
        .id("pull-request-controls")
        .gap_2()
        .children(pr.map(|pr| number(pr, work, cx)))
        .children(words::action(standing).map(|action| action_button(action, work, (busy, doing), on_action, cx)));
    Some(controls.into_any_element())
}

/// The pull request's number, outlined as a button that opens something is, which opens it on GitHub.
fn number(pr: &PullRequest, work: Work, cx: &App) -> AnyElement {
    let url = pr.url.clone();
    Button::new("pull-request")
        .hand()
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
    (busy, doing): (&Busy, Option<&'static str>),
    on_action: impl Fn(&Action, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    // Greyed out while the app is making, archiving or restoring a workspace, as every button that archives is, so it
    // can't be pressed twice, and while this one is on its way out, whoever asked. The other actions only ask the agent.
    let doing = doing.filter(|_| action == Action::Archive);
    let disabled = action == Action::Archive && (!busy.is_free() || doing.is_some());
    let label = doing.unwrap_or(action.label());
    let button = Button::new("pull-request-action").small().label(label).off_while(disabled);
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
