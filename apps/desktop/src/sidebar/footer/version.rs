use super::{controls, update_card};
use crate::app::{AppView, BuildLine};
use crate::checkout::Checkout;
use crate::updates::{Answer, Kind, Said};
use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, App, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, TestSupportExt as _, div,
};
use look::{IconSizing as _, Region, Shaped as _, TaskRing, Typeset as _};
use std::path::Path;

/// How tall the build line is, whatever it says, so a check for updates never moves the sidebar above it: a small
/// row's.
const HEIGHT: Pixels = look::RowSize::Small.height();

/// What pointing at the version's place shows: all of what it says, which it may cut short, or what goes with it.
enum Pointed {
    /// The words again, in a tooltip, since they're a few.
    Words,
    /// A card, with a heading and what's said under it, such as what went wrong.
    Card { heading: String, says: Vec<String> },
}

/// Something said about updating, in the version's place: an icon in a slot as wide as the ring, so the words start
/// in one place whichever it is, then the words, cut short before the buttons, and what pointing at it shows.
fn in_the_versions_place(
    id: &'static str,
    icon: impl IntoElement,
    words: impl Into<SharedString>,
    pointed: Pointed,
    color: Hsla,
) -> AnyElement {
    let words = words.into();
    let line = h_flex()
        .id(id)
        .min_w_0()
        .gap(look::Space::Small)
        .text_color(color)
        .child(h_flex().flex_none().w(look::Size::Inline.width()).justify_center().child(icon))
        .child(div().min_w_0().truncate().child(words.clone()));
    match pointed {
        Pointed::Words => line.tooltip(look::tooltip(words)).test_support().into_any_element(),
        Pointed::Card { heading, says } => over_the_line(line.test_support(), Some(heading), says),
    }
}

/// `line`, with the card that says `heading` and `says` above it once the pointer rests on it, from its left edge, so
/// it stays over the sidebar's foot rather than the build line's buttons.
fn over_the_line(line: impl IntoElement, heading: Option<String>, says: Vec<String>) -> AnyElement {
    let style = StyleRefinement::default().min_w_0().overflow_hidden();
    let card = move |cx: &mut App| crate::says_more::card(Region::BuildLineCard, heading, says, cx);
    crate::says_more::on_hover(look::hover_card("build-line-hover"), line, Anchor::BottomLeft, style, card)
        .into_any_element()
}

/// What a check is doing or found, or a problem updating. A check under way has a ring whose light goes round until
/// it's done, as a task's does while it runs. A download that isn't what the release workflow signed is the one
/// problem that's red, since it may be someone tampering with the update, and a problem's hover says the detail in
/// its own words too.
fn about_updating(said: Said<'_>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    match said {
        Said::Checking => {
            let words = "Checking for updates…";
            let ring = look::task_ring(TaskRing::Running, cx);
            in_the_versions_place("checking-for-updates", ring, words, Pointed::Words, colors.text_faint)
        }
        Said::Answer(answer) => {
            let icon = match answer {
                Answer::UpToDate => IconName::Check,
                Answer::NotUpdating => IconName::Info,
            };
            let words = answer.text();
            in_the_versions_place(
                "update-answer",
                Icon::new(icon).sized(look::IconSize::Small),
                words,
                Pointed::Words,
                colors.text_muted,
            )
        }
        Said::Problem(problem) => {
            let color = if matches!(problem.kind, Kind::Refused(_)) { colors.error } else { colors.text_muted };
            let (words, icon) = (problem.summary(), Icon::new(IconName::TriangleAlert).sized(look::IconSize::Small));
            let pointed = Pointed::Card { heading: words.clone(), says: vec![problem.detail.clone()] };
            in_the_versions_place("update-problem", icon, words, pointed, color)
        }
    }
}

/// The version, faint, and the hub's too when it's another build the app leaves alone.
fn version(line: &BuildLine, cx: &App) -> AnyElement {
    let text = div().id("version").truncate().text_color(look::colors(cx).text_faint);
    let BuildLine { app, other_hub, .. } = line;
    let Some(hub) = other_hub else { return text.child(app.clone()).test_support().into_any_element() };
    let says = vec![
        "The app leaves a hub someone started themselves for them to update, and never restarts a newer one."
            .to_string(),
    ];
    let heading = format!("The hub is {hub}, and this app is {app}");
    over_the_line(text.child(format!("{app} · hub {hub}")).test_support(), Some(heading), says)
}

/// Which checkout this build is from, and why it's open, so that several open at once can be told apart. Where it was
/// built, and where its hub keeps its data, are on hover.
fn checkout_card(checkout: &Checkout, data_dir: &Path, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let mut says = checkout.note().map(str::to_string).into_iter().collect::<Vec<_>>();
    says.push(format!("Built from {}", checkout.root().display()));
    says.push(format!("Its hub keeps its data in {}", data_dir.display()));
    let heading = checkout.name();
    let note = checkout.note().map(|note| {
        let text = div().id("checkout-note").typeset(look::Type::Detail).text_color(colors.text_muted).line_clamp(4);
        text.child(note.to_string()).test_support()
    });
    let card = v_flex()
        .id("checkout-card")
        .w_full()
        .p(look::Space::Medium)
        .gap(look::Space::XSmall)
        .corners(look::Corner::Card)
        .bg(colors.raised)
        .border_1()
        .border_color(colors.edge)
        .child(div().typeset(look::Type::Body).emphasis().truncate().child(checkout.name()))
        .when_some(note, ParentElement::child)
        .test_support();
    // Above the card, from its left edge, as wide as the sidebar's other cards' details.
    let details = move |cx: &mut App| crate::says_more::card(Region::CheckoutDetails, Some(heading), says, cx);
    let style = StyleRefinement::default().w_full();
    crate::says_more::on_hover(look::hover_card("checkout-hover"), card, Anchor::BottomLeft, style, details)
        .into_any_element()
}

/// The build line, with what's said about updating in the version's place while there's something to say, and its
/// two buttons side by side at the end, kept apart by their own room, so they read as a pair, and a message as long as
/// the update server's being out of reach fits whole before them.
fn build_line(line: &BuildLine, cx: &mut Context<AppView>) -> impl IntoElement {
    let said = crate::updates::said(cx).map_or_else(|| version(line, cx), |said| about_updating(said, cx));
    h_flex()
        .id("build-line")
        .w_full()
        .h(HEIGHT)
        .pl(look::Space::Small)
        .gap(look::Space::XSmall)
        .typeset(look::Type::Detail)
        .child(div().flex_1().min_w_0().child(said))
        .child(h_flex().flex_none().child(controls::help()).child(controls::settings(cx)))
        .test_support()
}

pub(in super::super) fn footer(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let build = app.build_line(cx);
    let border = look::colors(cx).edge;
    let checkout = app.checkout.as_ref().map(|checkout| checkout_card(checkout, &app.hub.paths.data_dir, cx));
    let card = update_card(&build, app.update_question.as_ref(), cx);
    v_flex()
        .flex_none()
        .p(look::Space::Small)
        .gap(look::Space::Small)
        .border_t_1()
        .border_color(border)
        .when_some(checkout, ParentElement::child)
        .when_some(card, ParentElement::child)
        .child(build_line(&build, cx))
}
