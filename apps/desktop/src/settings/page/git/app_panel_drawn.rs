//! The panel for making an app of your own, as it's drawn: its three steps, each with a check once it's done, or its
//! number, and what the one it's at has in it, under its line, from what the panel keeps and the hub's records of the
//! app and its installation. Cancel puts it away at any step, and Done once the app is installed.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use look::{IconSizing as _, Typeset as _};
use protocol::model::GithubAccount;

use super::super::field::labelled;
use super::Drawing;
use super::app_made::installation;
use super::app_panel::{AppPanel, AppStep, Choosing, OnGithub, listens_for};
use super::app_words;
use super::kept::{Kept, Panel};
use super::panel::{Framed, button, buttons, framed, refusal};

/// How far a step has got.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reached {
    Done,
    At,
    Later,
}

pub(super) fn app(drawing: &Drawing<'_>, panel: &Panel, app: &AppPanel, cx: &App) -> AnyElement {
    let made = match &app.step {
        AppStep::Installing { app, .. } => drawing.records.github_app(*app),
        AppStep::Choosing(_) | AppStep::OnGithub { .. } => None,
    };
    let installed = made.and_then(|made| installation(drawing.records, made.id));
    let bot = made.and_then(|made| drawing.records.github_app_bot(made)).map(|bot| bot.name.as_str());
    let lead = lead(drawing, installed, bot);
    let [first, second] = match (&app.step, made) {
        (_, Some(made)) => {
            app_words::made(&drawing.records.github_app_name(made), drawing.records.github_app_said(made))
        }
        (AppStep::OnGithub { chosen, .. }, None) => {
            [app_words::chosen(&chosen.under, &chosen.name), app_words::MAKE.to_string()]
        }
        (AppStep::Choosing(_) | AppStep::Installing { .. }, None) => {
            [app_words::CHOOSE.to_string(), app_words::MAKE.to_string()]
        }
    };
    let third = installed.and_then(|account| drawing.records.installed_on(account));
    let third = third.map_or(app_words::INSTALL.to_string(), |on| app_words::installed_on(&on.to_string()));
    let kept = drawing.kept;
    let steps = match &app.step {
        AppStep::Choosing(choosing) => [
            step(1, Reached::At, first, Some(choose(drawing, choosing, cx)), cx),
            step(2, Reached::Later, second, None, cx),
            step(3, Reached::Later, third, None, cx),
        ],
        AppStep::OnGithub { stands, .. } => [
            step(1, Reached::Done, first, None, cx),
            step(2, Reached::At, second, Some(on_github(drawing, stands, cx)), cx),
            step(3, Reached::Later, third, None, cx),
        ],
        AppStep::Installing { looking, failed, .. } => {
            let installing = (installed.is_none()).then(|| install(drawing, made, *looking, failed.clone(), cx));
            let third_reached = if installed.is_some() { Reached::Done } else { Reached::At };
            [
                step(1, Reached::Done, first, None, cx),
                step(2, Reached::Done, second, None, cx),
                step(3, third_reached, third, installing, cx),
            ]
        }
    };
    let end = if installed.is_some() {
        button("git-close", "Done", true, false, put_away(kept))
    } else {
        button("git-cancel", "Cancel", false, false, put_away(kept))
    };
    let framed_as = Framed {
        title: app_words::title(installed.is_some()).into(),
        lead: lead.into(),
        body: Some(v_flex().gap(look::Space::Large).children(steps).into_any_element()),
        refused: None,
        buttons: buttons().child(end),
    };
    framed(drawing, panel, framed_as, cx)
}

/// Puts the panel away, and whatever it waits on with it.
fn put_away(kept: &Entity<Kept>) -> impl Fn(&mut gpui_kit::Window, &mut App) + 'static {
    let kept = kept.clone();
    move |window, cx| kept.update(cx, |kept, cx| kept.put_away_panel(window, cx))
}

/// A step's line, `number`, with what's in it under it while the panel is at it. Assistive apps are told the line, and
/// whether it's done, and tests find it by `git-app-step-` and its number.
fn step(number: u8, reached: Reached, said: String, body: Option<AnyElement>, cx: &App) -> Div {
    let colors = look::colors(cx);
    let tile = div().flex_none().size(look::IconSize::Large.size()).flex().items_center().justify_center();
    let mark = match reached {
        Reached::Done => tile.child(Icon::new(IconName::Check).sized(look::IconSize::Large).text_color(colors.done)),
        Reached::At | Reached::Later => tile
            .rounded_full()
            .border(look::EDGE)
            .border_color(if reached == Reached::At { colors.text_muted } else { colors.edge })
            .typeset(look::Type::Detail)
            .text_color(if reached == Reached::At { colors.text } else { colors.text_faint })
            .child(number.to_string()),
    };
    let label = if reached == Reached::Done { format!("Done: {said}") } else { said.clone() };
    let line = div().id(SharedString::from(format!("git-app-step-{number}"))).role(Role::Label).aria_label(label);
    let line = line.text_color(match reached {
        Reached::Done => colors.text_muted,
        Reached::At => colors.text,
        Reached::Later => colors.text_faint,
    });
    h_flex().items_start().gap(look::Space::Medium).child(mark).child(
        v_flex().flex_1().min_w_0().gap(look::Space::Medium).child(line.child(said).test_support()).children(body),
    )
}

/// The first step: under the person's account, or an organization by its name, the app's name, and Make on GitHub.
fn choose(drawing: &Drawing<'_>, choosing: &Choosing, cx: &App) -> AnyElement {
    let under = |organization: bool| {
        let kept = drawing.kept.clone();
        move |window: &mut Window, cx: &mut App| {
            kept.update(cx, |kept, cx| kept.choose_organization(organization, window, cx));
        }
    };
    let organization = choosing.under_organization;
    let unders = look::switch(look::SwitchKind::Trackless)
        .words("git-app-under-account", "Your account", !organization, under(false))
        .words("git-app-under-organization", "An organization", organization, under(true));
    let organization = choosing.under_organization.then(|| {
        labelled("Organization", "git-app-organization", &choosing.organization.input, app_words::ORGANIZATION_SAYS, cx)
    });
    let name = labelled("Name", "git-app-name", &choosing.name.input, app_words::NAME_SAYS, cx);
    let making = drawing.kept.clone();
    let make = button("git-make-on-github", "Make on GitHub", true, !drawing.kept.read(cx).can_make_on_github(cx), {
        move |window, cx| making.update(cx, |kept, cx| kept.make_on_github(window, cx))
    });
    v_flex()
        .gap(look::Space::Large)
        .child(unders)
        .children(organization)
        .child(name)
        .children(choosing.refused.clone().map(|refused| refusal(refused, cx)))
        .child(h_flex().child(make))
        .into_any_element()
}

/// The second step, as making it on GitHub stands.
fn on_github(drawing: &Drawing<'_>, stands: &OnGithub, cx: &App) -> AnyElement {
    let kept = drawing.kept.clone();
    let off = !drawing.can_ask(cx);
    match stands {
        OnGithub::Opening => waiting_line(app_words::OPENING, cx).into_any_element(),
        OnGithub::Waiting { until_ms, .. } => {
            let [says, waiting] = app_words::on_github(look::time_of_day(*until_ms, cx).as_deref());
            let again = button("git-open-github", "Open GitHub again", false, false, move |_, cx| {
                kept.update(cx, |kept, cx| kept.open_github_again(cx));
            });
            let line = waiting_line(waiting, cx);
            column(says, cx).child(line).child(h_flex().child(again)).into_any_element()
        }
        OnGithub::StoppedWaiting => {
            let again = button("git-open-github", "Open GitHub again", true, off, move |window, cx| {
                kept.update(cx, |kept, cx| kept.open_github_from_the_start(window, cx));
            });
            let minutes = listens_for(cx).as_secs() / 60;
            v_flex()
                .gap(look::Space::Large)
                .child(refusal(app_words::stopped_waiting(minutes), cx))
                .child(h_flex().child(again))
                .into_any_element()
        }
        OnGithub::Making { failed: None, .. } => waiting_line(app_words::MAKING, cx).into_any_element(),
        OnGithub::Making { failed: Some(failed), .. } => {
            let again = if failed.again {
                button("git-try-again", "Try again", true, off, move |window, cx| {
                    kept.update(cx, |kept, cx| kept.make(window, cx));
                })
            } else {
                button("git-start-again", "Start again", true, false, move |window, cx| {
                    kept.update(cx, |kept, cx| kept.start_again(window, cx));
                })
            };
            let refused = refusal(failed.why.clone(), cx);
            v_flex().gap(look::Space::Large).child(refused).child(h_flex().child(again)).into_any_element()
        }
    }
}

/// The third step, while the app's installed nowhere: Install on GitHub, Check again, and how the hub's look went.
fn install(
    drawing: &Drawing<'_>,
    made: Option<&protocol::model::GithubApp>,
    looking: bool,
    failed: Option<String>,
    cx: &App,
) -> AnyElement {
    let Some(made) = made else {
        return refusal(app_words::gone(), cx).into_any_element();
    };
    let (installing, checking) = (drawing.kept.clone(), drawing.kept.clone());
    let (id, slug) = (made.id, drawing.records.github_app_said(made).map(|said| said.slug.clone()));
    let install = button("git-install", "Install on GitHub", true, false, move |window, cx| {
        installing.update(cx, |kept, cx| kept.install_on_github(id, slug.as_ref(), window, cx));
    })
    .icon(Lucide::ExternalLink);
    let check = button("git-check-again", "Check again", false, looking || !drawing.can_ask(cx), move |_, cx| {
        checking.update(cx, Kept::look_for_installations);
    });
    column(app_words::INSTALL_SAYS.to_string(), cx)
        .children(looking.then(|| waiting_line(app_words::LOOKING, cx)))
        .children(failed.map(|failed| refusal(failed, cx)))
        .child(h_flex().gap(look::Space::Small).child(install).child(check))
        .into_any_element()
}

/// What a step says, muted, with what follows it under it.
fn column(says: String, cx: &App) -> Div {
    v_flex().gap(look::Space::Large).child(div().text_color(look::colors(cx).text_muted).child(says))
}

/// That a step waits, beside a still circle.
fn waiting_line(said: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .gap(look::Space::Small)
        .text_color(look::colors(cx).text_muted)
        .child(Icon::new(Lucide::LoaderCircle).sized(look::IconSize::Regular).flex_none())
        .child(said.into())
}

/// What the panel says first: where the app's installation is, once there's one, and otherwise what making one does,
/// with its bot's name, once it has one.
fn lead(drawing: &Drawing<'_>, installed: Option<&GithubAccount>, bot: Option<&str>) -> String {
    let Some(account) = installed else { return app_words::lead(bot) };
    let on = drawing.records.installed_on(account).map_or_else(|| "GitHub".to_string(), |on| on.to_string());
    app_words::lead_installed(&drawing.records.github_username(account).to_string(), &on, drawing.is_default(account))
}
