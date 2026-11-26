use super::words;
use crate::busy::Busy;
use crate::sides;
use crate::sides::AtTheTop;
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, StyleRefinement,
    Styled as _, TestSupportExt as _, Window, div,
};
use look::{IconSizing as _, Sign, Typeset as _, Work};
use protocol::model::{OwnPullRequest, PullRequest, PullRequestFound, RepositoryUnseen, Standing};
use std::rc::Rc;
use words::Action;

use crate::warnings::{self, OpenGit, Warning};

/// What the app knows of a workspace's pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// Not asked yet. Saying so, rather than offering to create one, keeps a workspace that has one from offering it.
    Looking,
    /// Boxed, as a pull request is many times the size of the rest.
    Found(Option<Box<OwnPullRequest>>),
    /// GitHub showed the default no repository where the branch's pull requests are, and none was known before, so
    /// whether it has one can't be said. Boxed, as `Found` is.
    Unseen(Box<RepositoryUnseen>),
}

impl Lookup {
    /// What the hub `found`, which is `None` until it has looked, and whether GitHub showed it the repository, as
    /// `unseen` says. A pull request known before still shows, since it's still the workspace's, and a workspace's
    /// warning says it can't be read now.
    pub fn of(found: Option<PullRequestFound>, unseen: Option<RepositoryUnseen>) -> Self {
        match (found, unseen) {
            (Some(PullRequestFound { pull_request: Some(own) }), _) => Lookup::Found(Some(Box::new(own))),
            (_, Some(unseen)) => Lookup::Unseen(Box::new(unseen)),
            (Some(PullRequestFound { pull_request: None }), None) => Lookup::Found(None),
            (None, None) => Lookup::Looking,
        }
    }

    pub fn pull_request(&self) -> Option<&PullRequest> {
        match self {
            Lookup::Looking | Lookup::Unseen(_) => None,
            Lookup::Found(own) => own.as_ref().map(|own| &own.pull_request),
        }
    }

    /// `None` until it's known, and while it can't be.
    pub fn standing(&self) -> Option<Standing> {
        match self {
            Lookup::Looking | Lookup::Unseen(_) => None,
            Lookup::Found(None) => Some(Standing::NoPullRequest),
            Lookup::Found(Some(own)) => {
                Some(Standing::of(own.pull_request.said(), own.local, own.pull_request.checks_said()))
            }
        }
    }

    pub fn work(&self) -> Work {
        match self {
            Lookup::Looking | Lookup::Unseen(_) => Work::NoPr,
            Lookup::Found(pr) => client_ui::work(pr.as_deref()),
        }
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
    let on_action: OnAction = Rc::new(on_action);
    let row = look::bar(look::BarKind::Words, look::BarSize::Regular)
        .id("pull-request-bar")
        .clear(gpui_kit::px(0.), sides::clearance(AtTheTop::RightSidebar, cx).end)
        .of_work(work);
    if let Lookup::Unseen(unseen) = lookup {
        let open_git = on_action.clone();
        let words = unseen_words(unseen, Rc::new(move |window, cx| open_git(&Action::OpenGit, window, cx)), cx);
        return row
            .child(div().flex_1().min_w_0().child(words))
            .child(action_button(Action::OpenGit, work, (busy, doing), on_action))
            .into_any_element();
    }
    let Some(standing) = lookup.standing() else {
        return row
            .child(div().typeset(look::Type::Body).text_color(colors.text_faint).child("Looking for its pull request…"))
            .into_any_element();
    };
    let pr = lookup.pull_request();
    let title = client_core::standing::title(standing, pr.map(|pr| pr.said().base_branch.as_str()));
    let sign = client_ui::sign(standing);
    let text = if pr.is_some() { Sign::words(sign, work, colors) } else { colors.text_muted };
    let words = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(title);
    let title = h_flex()
        .flex_1()
        .min_w_0()
        .gap(look::Space::XSmall)
        .typeset(look::Type::Body)
        .emphasis()
        .text_color(text)
        .children(sign.map(|sign| signed(sign, work, cx)))
        .child(words);
    row.child(giving_way(pr.map(|pr| number(pr, work)), title))
        .children(words::action(standing).map(|action| action_button(action, work, (busy, doing), on_action)))
        .into_any_element()
}

/// What the bar does with its action.
type OnAction = Rc<dyn Fn(&Action, &mut Window, &mut App)>;

/// That the pull request can't be seen, after the warning's sign, in its colour, which opens the workspace's warning
/// about it, its Open Settings doing what `open_git` does.
fn unseen_words(unseen: &RepositoryUnseen, open_git: OpenGit, cx: &App) -> AnyElement {
    let warnings = vec![Warning::unseen(unseen)];
    let words = h_flex()
        .id("pull-request-unseen")
        .gap(look::Space::XSmall)
        .typeset(look::Type::Body)
        .emphasis()
        .text_color(look::colors(cx).warning)
        .child(warnings::sign(cx))
        .child(
            div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child("Can't see its pull request"),
        )
        .test_support();
    look::hover_card_with_a_button("pull-request-unseen-card")
        .anchor(Anchor::TopLeft)
        .trigger_style(StyleRefinement::default().min_w_0().overflow_hidden())
        .trigger(words)
        .content(move |_, _, cx| warnings::card(&warnings, &open_git, cx))
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
/// sidebar has room for and the header doesn't. What's in the way shows by its sign beside the number, so a problem
/// shows there too. Nothing while it can't be seen, since the workspace's name has the warning's sign beside it then.
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
    let sign = pr.and(client_ui::sign(standing));
    let controls = h_flex()
        .id("pull-request-controls")
        .gap(look::Space::Small)
        .children(sign.map(|sign| signed(sign, work, cx)))
        .children(pr.map(|pr| number(pr, work)))
        .children(words::action(standing).map(|action| action_button(action, work, (busy, doing), Rc::new(on_action))));
    Some(controls.into_any_element())
}

/// The icon of what's in the way, `sign`, on a bar of `work`'s colour.
fn signed(sign: Sign, work: Work, cx: &App) -> Icon {
    let size = look::ControlSize::Regular;
    Icon::new(sign.icon()).text_color(sign.color(work, look::colors(cx))).sized(size.icon_with_words()).flex_none()
}

/// The pull request's number, outlined as a button that opens something is, which opens it on GitHub.
fn number(pr: &PullRequest, work: Work) -> AnyElement {
    let about = &pr.said().about;
    let url = about.url.clone();
    let size = look::ControlSize::Regular;
    look::button("pull-request", look::ButtonKind::Outlined, size, format!("#{}", pr.number.get()))
        .of_work(work)
        .tooltip(about.title.clone())
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
    on_action: OnAction,
) -> AnyElement {
    // Greyed out while the app is making, archiving or restoring a workspace, as every button that archives is, so it
    // can't be pressed twice, and while this one is on its way out, whoever asked. The others ask the agent or open Settings,
    // which nothing holds up.
    let doing = doing.filter(|_| action == Action::Archive);
    let disabled = action == Action::Archive && (!busy.is_free() || doing.is_some());
    let label = doing.unwrap_or(action.label());
    look::button("pull-request-action", look::ButtonKind::Main, look::ControlSize::Regular, label)
        .of_work(work)
        .off(disabled)
        .on_click(move |_, window, cx| on_action(&action, window, cx))
        .into_any_element()
}
