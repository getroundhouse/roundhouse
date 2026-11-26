//! A reference's card, which tells what it is and where it stands, and its menu, the same on every chip.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Anchor, AnyElement, App, ClipboardItem, IntoElement, ParentElement as _, Styled as _, div, rgb};
use look::{CardKind, CardWidth, GithubState, IssueState, Menu, PullRequestState, Region, Space, Type, Typeset as _};
use protocol::model::{About, ChecksRollup, QueueState, ReferenceSaid, Review};

use super::kept::{References, Tie};
use crate::linked_images::Waiter;

/// The design system's card over the rest: its title as its heading, with the button that opens its menu, where it
/// stands under it, who opened it and when, then its labels, where a pull request's checks stand, the workspace it's
/// in, and the start of what it says.
pub(super) fn card<W: Waiter>(
    reference: &ReferenceSaid,
    state: GithubState,
    tie: &Tie,
    references: &References<W>,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let detail = || div().flex().flex_wrap().items_center().gap(Space::XSmall).typeset(Type::Detail);
    // One this version can't draw has no chip to rest on, so this is never shown.
    let (Some(about), Some(ask)) = (reference.about(), reference.asked()) else { return div().into_any_element() };
    let number = ask.number;
    let pull_request = match reference {
        ReferenceSaid::PullRequest(pull_request) => Some(&pull_request.summary),
        ReferenceSaid::Issue(_) | ReferenceSaid::Missing | ReferenceSaid::Unknown => None,
    };
    let (kind, word) = said_of(state, pull_request.and_then(|pull_request| pull_request.queue));
    let stands = detail()
        .text_color(colors.text_faint)
        .child(div().text_color(state.color(cx)).child(word))
        .child("·")
        .child(format!("{kind} #{number}"));
    let said = detail().text_color(colors.text_faint).child(opened(about, cx)).when_some(
        pull_request,
        |line, pull_request| {
            line.child("·").child(look::lines_changed(
                look::LinesKind::Plain,
                pull_request.added as usize,
                pull_request.removed as usize,
            ))
        },
    );
    let labels = (!about.labels.is_empty()).then(|| {
        div().flex().flex_wrap().gap(Space::XSmall).children(about.labels.iter().map(|label| {
            // GitHub keeps six hex digits. One that isn't is drawn in the muted colour of text that says little.
            let color = u32::from_str_radix(&label.color, 16).map_or(colors.text_muted, |color| rgb(color).into());
            look::label(label.name.clone(), color)
        }))
    });
    let checks = pull_request.and_then(|pull_request| {
        let words = [checks_words(pull_request.rollup), review_words(pull_request.review)];
        let words: Vec<&str> = words.into_iter().flatten().collect();
        (!words.is_empty()).then(|| detail().child(words.join(" · ")))
    });
    let tied = tie_line(tie, cx);
    let body = (!about.body.is_empty()).then(|| {
        div()
            .typeset(Type::Detail)
            .text_color(colors.text_muted)
            .line_clamp(3)
            .text_ellipsis()
            .child(about.body.clone())
    });
    let (for_menu, tie, references) = (reference.clone(), tie.clone(), references.clone());
    let heading = look::card_heading(about.title.clone()).mark(state.icon()).control(
        look::icon_button("reference-more", look::ButtonKind::Faint, look::ControlSize::Small, IconName::Ellipsis)
            .tooltip("More")
            .menu(Anchor::TopRight, move |list, _, cx| menu(&for_menu, &tie, &references, list, cx)),
    );
    look::card(CardKind::Over, CardWidth::Regular)
        .region(Region::ReferenceCard)
        .heading(heading)
        .under_heading(stands)
        .under_heading(said)
        .children(labels)
        .children(checks)
        .children(tied)
        .children(body)
        .into_any_element()
}

/// What it is, and the word for where it stands, which the card says in its state's colour. One in the merge queue,
/// at `queue`, says where it is in it as the bar does.
pub(super) fn said_of(state: GithubState, queue: Option<QueueState>) -> (&'static str, &'static str) {
    match state {
        GithubState::Issue(state) => (
            "Issue",
            match state {
                IssueState::Open => "Open",
                IssueState::Done => "Closed",
                IssueState::NotPlanned => "Not planned",
            },
        ),
        GithubState::PullRequest(state) => (
            "Pull request",
            match state {
                PullRequestState::Open => "Open",
                PullRequestState::Draft => "Draft",
                // Only one in the queue is drawn as queued, so it has a step, and without one it says what the bar
                // says of a step it doesn't know.
                PullRequestState::Queued => client_core::standing::queued(queue.unwrap_or(QueueState::Unknown)),
                PullRequestState::Merged => "Merged",
                PullRequestState::Closed => "Closed",
            },
        ),
    }
}

/// Who opened it and when, and how much has been said on it, such as `ada opened this 3 days ago · 6 comments`.
fn opened(about: &About, cx: &App) -> String {
    let who = about.author.as_deref().unwrap_or("Someone");
    let mut said = format!("{who} opened this");
    if let Some(when) = look::day_ago(about.opened_at_ms, look::now_ms(cx), cx) {
        said = format!("{said} {when}");
    }
    match about.comments {
        0 => said,
        1 => format!("{said} · 1 comment"),
        many => format!("{said} · {} comments", written_counts::whole(many)),
    }
}

fn checks_words(checks: Option<ChecksRollup>) -> Option<&'static str> {
    match checks? {
        ChecksRollup::Passed => Some("Checks passed"),
        ChecksRollup::Failed => Some("Checks failed"),
        ChecksRollup::Running => Some("Checks running"),
        ChecksRollup::Unknown => None,
    }
}

fn review_words(review: Review) -> Option<&'static str> {
    match review {
        Review::Approved => Some("Approved"),
        Review::ChangesRequested => Some("Changes requested"),
        Review::Required => Some("Waiting on a review"),
        Review::NotRequired | Review::Unknown => None,
    }
}

/// Which workspace it's in, as a line of the card. An archived one is only said, since it's brought back from Home.
fn tie_line(tie: &Tie, cx: &App) -> Option<AnyElement> {
    let colors = look::colors(cx);
    let (words, name, after, branch) = match tie {
        Tie::None => return None,
        Tie::ThisOne { branch } => ("This workspace's, on", None, "", branch),
        Tie::Elsewhere { name, branch, archived: false, .. } => ("In", Some(format!("{name},")), "on", branch),
        Tie::Elsewhere { name, branch, archived: true, .. } => {
            ("In", Some(format!("{name},")), "which is archived, on", branch)
        }
    };
    Some(
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(Space::XSmall)
            .typeset(Type::Detail)
            .text_color(colors.text_muted)
            .child(words)
            .when_some(name, |line, name| line.child(div().text_color(colors.text).child(name)))
            .when(!after.is_empty(), |line| line.child(after))
            .child(div().typeset_code(cx).text_color(colors.text_faint).child(branch.clone()))
            .into_any_element(),
    )
}

/// The chip's menu, the same rows on every chip, with what doesn't apply to this one off, and Copy link first under
/// touch, as the menu puts every row that copies. An archived workspace is named but can't be gone to, since it's
/// brought back from Home.
pub(super) fn menu<W: Waiter>(
    reference: &ReferenceSaid,
    tie: &Tie,
    references: &References<W>,
    list: Menu,
    _: &mut App,
) -> Menu {
    let url = reference.about().map_or_default(|about| about.url.clone());
    let (open, copy) = (url.clone(), url);
    let (go, to) = match tie {
        Tie::Elsewhere { id, name, archived, .. } => (format!("Go to {name}"), (!archived).then(|| id.clone())),
        Tie::ThisOne { .. } | Tie::None => ("Go to its workspace".to_string(), None),
    };
    let references = references.clone();
    let open = look::menu_row("Open on GitHub").icon(IconName::ArrowUpRight).on_choose(move |_, cx| cx.open_url(&open));
    let copy = look::menu_row("Copy link")
        .icon(IconName::Link)
        .copies()
        .on_choose(move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())));
    list.row(open).row(copy).separator().row(look::menu_row(go).icon(IconName::GitBranch).off(to.is_none()).on_choose(
        move |_, cx| {
            if let Some(to) = &to {
                references.go_to(to.clone(), cx);
            }
        },
    ))
}
