use super::words;
use crate::busy::Busy;
use crate::sides;
use crate::sides::AtTheTop;
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, Window, div,
};
use look::{IconSizing as _, Typeset as _, Work};
use protocol::model::{BranchName, PullRequest, PullRequestFound, Standing};
use words::Action;

/// What the app knows of a workspace's pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// Not asked yet. Saying so, rather than offering to create one, keeps a workspace that has one from offering it.
    Looking,
    Found(Option<PullRequest>),
}

impl Lookup {
    /// What the hub `found`, which is `None` until it has looked.
    pub fn of(found: Option<PullRequestFound>) -> Self {
        found.map_or(Lookup::Looking, |found| Lookup::Found(found.pull_request))
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
        self.standing().map_or(Work::NoPr, client_ui::work)
    }
}

/// The bar at the top of the sidebar: the pull request's number, which opens it, what stands in its way, and the
/// one thing to do about it, all in its colour. `busy` greys out Archive, as every button that archives is while the
/// app is making, archiving or restoring a workspace, and it says what the workspace it's for is `doing` in its place,
/// such as `Archiving…`, as [`client_core::moving::doing`] reads it from its record.
pub fn bar(
    lookup: &Lookup,
    busy: &Busy,
    doing: Option<&'static str>,
    on_action: impl Fn(&Action, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let work = lookup.work();
    let colors = look::colors(cx);
    let row = look::bar(look::BarKind::Words, look::BarSize::Regular)
        .id("pull-request-bar")
        .clear(gpui_kit::px(0.), sides::clearance(AtTheTop::RightSidebar, cx).end)
        .of_work(work);
    let Some(standing) = lookup.standing() else {
        return row
            .child(div().typeset(look::Type::Body).text_color(colors.text_faint).child("Looking for its pull request…"))
            .into_any_element();
    };
    let pr = lookup.pull_request();
    let title = client_core::standing::title(standing, pr.and_then(|pr| pr.base.as_ref()).map(BranchName::as_str));
    let text = if pr.is_some() { work.text(colors) } else { colors.text_muted };
    let title = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .typeset(look::Type::Body)
        .emphasis()
        .text_color(text)
        .child(title);
    row.child(giving_way(pr.map(|pr| number(pr, work)), title))
        .children(words::action(standing).map(|action| action_button(action, work, (busy, doing), on_action)))
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
        .gap(look::Space::Small)
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
) -> Option<AnyElement> {
    let standing = lookup.standing()?;
    let work = lookup.work();
    let pr = lookup.pull_request();
    let controls = h_flex()
        .id("pull-request-controls")
        .gap(look::Space::Small)
        .children(pr.map(|pr| number(pr, work)))
        .children(words::action(standing).map(|action| action_button(action, work, (busy, doing), on_action)));
    Some(controls.into_any_element())
}

/// The pull request's number, outlined as a button that opens something is, which opens it on GitHub.
fn number(pr: &PullRequest, work: Work) -> AnyElement {
    let url = pr.url.clone();
    let size = look::ControlSize::Regular;
    look::button("pull-request", look::ButtonKind::Outlined, size, format!("#{}", pr.number))
        .of_work(work)
        .tooltip(pr.title.clone())
        .after(Icon::new(Lucide::ArrowUpRight).sized(size.icon_with_words()))
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
) -> AnyElement {
    // Greyed out while the app is making, archiving or restoring a workspace, as every button that archives is, so it
    // can't be pressed twice, and while this one is on its way out, whoever asked. The other actions only ask the agent.
    let doing = doing.filter(|_| action == Action::Archive);
    let disabled = action == Action::Archive && (!busy.is_free() || doing.is_some());
    let label = doing.unwrap_or(action.label());
    look::button("pull-request-action", look::ButtonKind::Main, look::ControlSize::Regular, label)
        .of_work(work)
        .off(disabled)
        .on_click(move |_, window, cx| on_action(&action, window, cx))
        .into_any_element()
}
