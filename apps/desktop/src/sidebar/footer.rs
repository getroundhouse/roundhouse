//! The bottom of the sidebar: adding a repository, the archive, and the build line, with where updating has got to.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, TestSupportExt as _, div, px,
};
use look::{IconSize, IconStyle};

use crate::app::{AppView, BuildLine, UpdateQuestion};
use crate::updates::{Answer, Kind, Problem};

fn footer_item(id: &'static str, icon: IconName, label: &'static str) -> Button {
    Button::new(id).ghost().small().icon(icon).label(label)
}

/// What went wrong updating, under the build, with the detail in its own words on hover. A download that isn't what
/// the release workflow signed is the one that's red, since it may be someone tampering with the update.
fn update_problem(problem: &Problem, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let color = if matches!(problem.kind, Kind::Refused(_)) { theme.danger } else { theme.muted_foreground };
    let detail = problem.detail.clone();
    h_flex()
        .id("update-problem")
        .px_2()
        .gap_1p5()
        .text_xs()
        .text_color(color)
        .child(Icon::new(IconName::TriangleAlert).xsmall())
        .child(problem.summary())
        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
        .test_support()
        .into_any_element()
}

/// The version, faint, and the hub's too when it's another build the app leaves alone.
fn version(line: &BuildLine) -> AnyElement {
    let text = div().id("build-line").px_2().pt_1().text_xs().text_color(look::faint());
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

/// Where an update has got to, when there's something to say, as a card with the app's icon in the version's place,
/// which says the version on hover. Installing an update is the card's main action. Finishing one stops agents, and
/// happens by itself, so it's a quiet button under what it's waiting on. When either would lose work, the card asks
/// first, as its title, in place of its buttons.
fn update_card(line: &BuildLine, question: Option<&UpdateQuestion>, cx: &mut Context<AppView>) -> Option<AnyElement> {
    let (about, action) = match line {
        BuildLine::UpdateReady { app, next } => {
            ready_card(app, next, question.filter(|q| matches!(q, UpdateQuestion::Restart { .. })), cx)
        }
        BuildLine::Finishing { app, hub, agents } => {
            finishing_card(app, hub, *agents, question.filter(|q| matches!(q, UpdateQuestion::Finish(_))), cx)
        }
        BuildLine::Current { .. } | BuildLine::OtherHub { .. } => return None,
    };
    let theme = cx.theme();
    let card = v_flex()
        .id("update-card")
        .w_full()
        .mt_2()
        .p_2p5()
        .gap_2p5()
        .rounded(px(10.))
        .bg(theme.secondary)
        .border_1()
        .border_color(theme.border)
        .child(h_flex().items_start().gap_2p5().child(look::icon(IconStyle::Working, IconSize::Px64, cx)).child(about))
        .when_some(action, ParentElement::child)
        .test_support();
    Some(card.into_any_element())
}

/// What the card says beside the icon: its title, a detail under it, and more on hover.
fn card_text(title: &str, detail: String, hover: String, cx: &App) -> Stateful<Div> {
    v_flex()
        .id("update-about")
        .min_w_0()
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title.to_string()))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(detail))
        .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
}

/// An update is downloaded, and restarting installs it, unless the card is asking first.
fn ready_card(
    app: &str,
    next: &str,
    question: Option<&UpdateQuestion>,
    cx: &mut Context<AppView>,
) -> (Stateful<Div>, Option<AnyElement>) {
    let hover = format!(
        "Roundhouse {next} is downloaded, and this is {app}. Restarting installs it, and agents keep running. \
         Quitting installs it too."
    );
    let title = question.map_or("Update ready", UpdateQuestion::title);
    let about = card_text(title, format!("Roundhouse {next}"), hover, cx);
    let action = if let Some(question) = question {
        asking(question, "Keep working", "Restart", AppView::restart_to_update_anyway, cx)
    } else {
        let restart = cx.listener(|t, _, _, cx| t.restart_to_update(cx));
        Button::new("restart-to-update")
            .primary()
            .small()
            .w_full()
            .label("Restart to update")
            .on_click(restart)
            .into_any_element()
    };
    (about, Some(action))
}

/// The app has updated, and the older hub is still busy, so it finishes by itself later, or now if asked.
fn finishing_card(
    app: &str,
    hub: &str,
    agents: usize,
    question: Option<&UpdateQuestion>,
    cx: &mut Context<AppView>,
) -> (Stateful<Div>, Option<AnyElement>) {
    let hover = if agents == 0 {
        format!(
            "This is {app}, and the previous version, {hub}, is still busy, such as with a setup script or the \
             command line connected to it. The update finishes by itself once it's free."
        )
    } else {
        format!(
            "This is {app}, and agents are still running on the previous version, {hub}. The update finishes by \
             itself once they're done."
        )
    };
    if let Some(question) = question {
        let about = card_text(question.title(), waiting_on(agents), hover, cx);
        return (about, Some(asking(question, "Not now", "Finish now", AppView::finish_update_now, cx)));
    }
    // Under the text, and in line with it, past the button's own padding.
    let finish = h_flex().pt_1().ml(px(-3.5)).child(finish_now(cx));
    (card_text("Finishing update", waiting_on(agents), hover, cx).child(finish), None)
}

/// What the card asks before going ahead: what that loses, then a quiet button that leaves things as they are, and
/// the main one, which goes ahead.
fn asking(
    question: &UpdateQuestion,
    stay: &'static str,
    go: &'static str,
    go_ahead: fn(&mut AppView, &mut Context<AppView>),
    cx: &mut Context<AppView>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let stay = Button::new("update-not-now")
        .ghost()
        .small()
        .flex_1()
        .label(stay)
        .on_click(cx.listener(|t, _, _, cx| t.dismiss_update_question(cx)));
    let go = Button::new("update-go-ahead")
        .primary()
        .small()
        .flex_1()
        .label(go)
        .on_click(cx.listener(move |t, _, _, cx| go_ahead(t, cx)));
    v_flex()
        .id("update-question")
        .gap_2p5()
        .child(div().text_xs().text_color(muted).child(question.detail()))
        .child(h_flex().gap_2().child(stay).child(go))
        .test_support()
        .into_any_element()
}

/// What finishing an update is waiting on.
fn waiting_on(agents: usize) -> String {
    match agents {
        0 => "Waiting on other work".to_string(),
        1 => "Waiting on 1 agent".to_string(),
        n => format!("Waiting on {n} agents"),
    }
}

/// What a check for updates is doing or found, faint, under the build.
fn check_line(id: &'static str, text: &'static str) -> AnyElement {
    div().id(id).px_2().text_xs().text_color(look::faint()).child(text).test_support().into_any_element()
}

/// A quiet button, since finishing an update now stops the agents it's waiting on, which the app says first.
fn finish_now(cx: &mut Context<AppView>) -> Button {
    let finish = cx.listener(|t, _, _, cx| t.offer_hub_restart(cx));
    Button::new("finish-update").ghost().xsmall().label("Finish now").on_click(finish)
}

pub(super) fn footer(
    show_archived: bool,
    build: &BuildLine,
    question: Option<&UpdateQuestion>,
    checking: bool,
    answer: Option<Answer>,
    update: Option<&Problem>,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let border = cx.theme().sidebar_border;
    let card = update_card(build, question, cx);
    let problem = update.map(|problem| update_problem(problem, cx));
    v_flex()
        .flex_none()
        .items_start()
        .p_2()
        .gap_px()
        .border_t_1()
        .border_color(border)
        .child(
            footer_item("add-repository", IconName::Plus, "Add repository")
                .on_click(cx.listener(|t, _, _, cx| t.add_repository(cx))),
        )
        .child(
            footer_item("toggle-archived", IconName::Inbox, if show_archived { "Hide archived" } else { "Archived" })
                .on_click(cx.listener(|t, _, _, cx| {
                    t.show_archived = !t.show_archived;
                    cx.notify();
                })),
        )
        .child(card.unwrap_or_else(|| version(build)))
        .when(checking, |d| d.child(check_line("checking-for-updates", "Checking for updates…")))
        .when_some(answer, |d, answer| d.child(check_line("update-answer", answer.text())))
        .when_some(problem, ParentElement::child)
}
