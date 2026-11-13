use super::{controls, update_card};
use crate::app::{AppView, BuildLine};
use crate::checkout::Checkout;
use crate::updates::{Answer, Kind, Said};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt as _, div, px,
};
use look::{Shaped as _, TaskRing, Typeset as _};
use std::path::Path;

/// How tall the build line is, whatever it says, so a check for updates never moves the sidebar above it. As tall as
/// its buttons and a little room around them.
const HEIGHT: Pixels = px(28.);

/// Something said about updating, in the version's place: an icon in a slot as wide as the ring, so the words start
/// in one place whichever it is, then the words, cut short before the buttons, and on hover `hover`, which has all of
/// them.
fn in_the_versions_place(
    id: &'static str,
    icon: impl IntoElement,
    words: impl Into<SharedString>,
    hover: String,
    color: Hsla,
) -> AnyElement {
    h_flex()
        .id(id)
        .min_w_0()
        .gap_1p5()
        .text_color(color)
        .child(h_flex().flex_none().w(look::Size::Inline.width()).justify_center().child(icon))
        .child(div().min_w_0().truncate().child(words.into()))
        .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
        .test_support()
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
            in_the_versions_place("checking-for-updates", ring, words, words.to_string(), colors.text_faint)
        }
        Said::Answer(answer) => {
            let icon = match answer {
                Answer::UpToDate => IconName::Check,
                Answer::NotUpdating => IconName::Info,
            };
            let words = answer.text();
            in_the_versions_place(
                "update-answer",
                Icon::new(icon).xsmall(),
                words,
                words.to_string(),
                colors.text_muted,
            )
        }
        Said::Problem(problem) => {
            let color = if matches!(problem.kind, Kind::Refused(_)) { colors.error } else { colors.text_muted };
            let (words, icon) = (problem.summary(), Icon::new(IconName::TriangleAlert).xsmall());
            let hover = format!("{words}\n\n{}", problem.detail);
            in_the_versions_place("update-problem", icon, words, hover, color)
        }
    }
}

/// The version, faint, and the hub's too when it's another build the app leaves alone.
fn version(line: &BuildLine, cx: &App) -> AnyElement {
    let text = div().id("version").truncate().text_color(look::colors(cx).text_faint);
    let BuildLine { app, other_hub, .. } = line;
    let Some(hub) = other_hub else { return text.child(app.clone()).test_support().into_any_element() };
    let detail = format!(
        "The hub is {hub}, and this app is {app}. The app leaves a hub someone started themselves for them to update, \
         and never restarts a newer one."
    );
    text.child(format!("{app} · hub {hub}"))
        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
        .test_support()
        .into_any_element()
}

/// Which checkout this build is from, and why it's open, so that several open at once can be told apart. Where it was
/// built, and where its hub keeps its data, are on hover.
fn checkout_card(checkout: &Checkout, data_dir: &Path, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let built = format!("Built from {}. Its hub keeps its data in {}.", checkout.root().display(), data_dir.display());
    let hover = checkout.note().map_or_else(|| built.clone(), |note| format!("{note}\n\n{built}"));
    let note = checkout.note().map(|note| {
        let text = div().id("checkout-note").typeset(look::Type::Detail).text_color(colors.text_muted).line_clamp(4);
        text.child(note.to_string()).test_support()
    });
    v_flex()
        .id("checkout-card")
        .w_full()
        .p_2p5()
        .gap_0p5()
        .corners(look::Corner::Card)
        .bg(colors.raised)
        .border_1()
        .border_color(colors.edge)
        .child(div().typeset(look::Type::Body).emphasis().truncate().child(checkout.name()))
        .when_some(note, ParentElement::child)
        .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
        .test_support()
        .into_any_element()
}

/// The build line, with what's said about updating in the version's place while there's something to say.
fn build_line(line: &BuildLine, cx: &mut Context<AppView>) -> impl IntoElement {
    let said = crate::updates::said(cx).map_or_else(|| version(line, cx), |said| about_updating(said, cx));
    h_flex()
        .id("build-line")
        .w_full()
        .h(HEIGHT)
        .pl_2()
        .gap_0p5()
        .typeset(look::Type::Detail)
        .child(div().flex_1().min_w_0().child(said))
        .child(controls::help(cx))
        .child(controls::settings(cx))
        .test_support()
}

pub(in super::super) fn footer(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let build = app.build_line(cx);
    let border = look::colors(cx).edge;
    let checkout = app.checkout.as_ref().map(|checkout| checkout_card(checkout, &app.hub.paths.data_dir, cx));
    let card = update_card(&build, app.update_question.as_ref(), cx);
    v_flex()
        .flex_none()
        .p_2()
        .gap_2()
        .border_t_1()
        .border_color(border)
        .when_some(checkout, ParentElement::child)
        .when_some(card, ParentElement::child)
        .child(build_line(&build, cx))
}
