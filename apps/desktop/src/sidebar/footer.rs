//! The bottom of the sidebar: the build line, with where updating has got to, and which checkout a build from one is
//! from.

use std::path::Path;

mod update_card;
mod update_details;

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    TestSupportExt as _, div, px,
};
use look::Typeset as _;

use crate::app::{AppView, BuildLine};
use crate::checkout::Checkout;
use crate::updates::{Kind, Problem};
use update_card::update_card;

/// What went wrong updating, under the build, with the detail in its own words on hover. A download that isn't what
/// the release workflow signed is the one that's red, since it may be someone tampering with the update.
fn update_problem(problem: &Problem, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let color = if matches!(problem.kind, Kind::Refused(_)) { colors.error } else { colors.text_muted };
    let detail = problem.detail.clone();
    h_flex()
        .id("update-problem")
        .px_2()
        .gap_1p5()
        .typeset(look::Type::Detail)
        .text_color(color)
        .child(Icon::new(IconName::TriangleAlert).xsmall())
        .child(problem.summary())
        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
        .test_support()
        .into_any_element()
}

/// The version, faint, and the hub's too when it's another build the app leaves alone.
fn version(line: &BuildLine, cx: &App) -> AnyElement {
    let text = div().id("build-line").px_2().pt_1().typeset(look::Type::Detail).text_color(look::colors(cx).text_faint);
    match line {
        BuildLine::OtherHub { app, hub } => {
            let detail = format!(
                "The hub is {hub}, and this app is {app}. The app leaves a hub someone started themselves for them to \
                 update, and never restarts a newer one."
            );
            text.child(format!("{app} · hub {hub}"))
                .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
                .test_support()
                .into_any_element()
        }
        BuildLine::Current { app } | BuildLine::UpdateReady { app, .. } | BuildLine::Finishing { app, .. } => {
            text.child(app.clone()).test_support().into_any_element()
        }
    }
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
        .rounded(px(10.))
        .bg(colors.raised)
        .border_1()
        .border_color(colors.edge)
        .child(div().typeset(look::Type::Body).emphasis().truncate().child(checkout.name()))
        .when_some(note, ParentElement::child)
        .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
        .test_support()
        .into_any_element()
}

/// What a check for updates is doing or found, faint, under the build.
fn check_line(id: &'static str, text: &'static str, cx: &App) -> AnyElement {
    div()
        .id(id)
        .px_2()
        .typeset(look::Type::Detail)
        .text_color(look::colors(cx).text_faint)
        .child(text)
        .test_support()
        .into_any_element()
}

pub(super) fn footer(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let build = app.build_line(cx);
    let border = look::colors(cx).edge;
    let checkout = app.checkout.as_ref().map(|checkout| checkout_card(checkout, &app.hub.paths.data_dir, cx));
    let card = update_card(&build, app.update_question.as_ref(), cx);
    let problem = crate::updates::problem_shown(cx).cloned().map(|problem| update_problem(&problem, cx));
    let (checking, answer) = (crate::updates::checking(cx), crate::updates::answered(cx));
    v_flex()
        .flex_none()
        .items_start()
        .p_2()
        .gap_px()
        .border_t_1()
        .border_color(border)
        .when_some(checkout, ParentElement::child)
        .child(card.unwrap_or_else(|| version(&build, cx)))
        .when(checking, |d| d.child(check_line("checking-for-updates", "Checking for updates…", cx)))
        .when_some(answer, |d, answer| d.child(check_line("update-answer", answer.text(), cx)))
        .when_some(problem, ParentElement::child)
}
