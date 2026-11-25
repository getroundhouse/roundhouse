//! The panels for connecting a GitHub account: Connect account's, with the ways to connect one, and the one a token is
//! pasted into, for a new account or in place of an account's own login. The panel for making an app of your own is
//! `app_panel`'s.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
};
use look::{Hand as _, IconSizing as _};

use super::super::field::labelled;
use super::Drawing;
use super::gh_choice::ChoosingPanel;
use super::kept::{Kept, Panel};
use super::panel::{Framed, button, buttons, choice, choices, framed};
use super::token::TokenPanel;
use super::{app_words, words};

/// The guide's page on who agents are on GitHub, which Connect account's panel links to.
const WHICH: &str = "https://getroundhouse.dev/docs/concepts/who-agents-are-on-github/";

/// Where GitHub makes a fine-grained token.
const MAKE_A_TOKEN: &str = "https://github.com/settings/personal-access-tokens/new";

/// Who agents act as with an account the person connects as themselves.
const AS_YOU: &str = "Agents act as you";

/// Connect account, open: each way to connect an account, with who agents act as and what it takes, `gh`'s login on
/// the hub's machine first, once the hub has said it's logged in, and while no account holds it.
pub(super) fn choosing(drawing: &Drawing<'_>, panel: &Panel, choosing: &ChoosingPanel, cx: &App) -> AnyElement {
    let off = !drawing.can_ask(cx);
    let (signing_in, pasting, making) = (drawing.kept.clone(), drawing.kept.clone(), drawing.kept.clone());
    let sign_in = button("git-sign-in", "Sign in", false, off, move |_, cx| {
        signing_in.update(cx, |kept, cx| kept.sign_in(None, cx));
    });
    let token = button("git-use-token", "Paste a token", false, off, move |window, cx| {
        pasting.update(cx, |kept, cx| kept.open_token(None, window, cx));
    });
    let name = app_words::default_name(drawing.records, drawing.github.and_then(|github| github.account.as_ref()));
    let app = button("git-make-app", "Make an app", false, off, move |window, cx| {
        making.update(cx, |kept, cx| kept.open_app(&name, window, cx));
    });
    let gh = choosing.offered(drawing.records).map(|username| {
        let using = drawing.kept.clone();
        let act =
            button("git-use-gh", words::use_gh(username), false, off || choosing.connecting(), move |window, cx| {
                using.update(cx, |kept, cx| kept.use_gh(window, cx));
            });
        choice(Lucide::SquareTerminal, "Use gh's login", Some(AS_YOU), words::gh_choice_says(username), act, cx)
    });
    let ways = [
        choice(
            Lucide::Github,
            "Sign in with GitHub",
            Some(AS_YOU),
            "Enter a code on GitHub, and the hub renews the login by itself. An organization that limits which apps \
             can reach it has to approve Roundhouse first.",
            sign_in,
            cx,
        ),
        choice(
            Lucide::KeyRound,
            "Use a token",
            Some(AS_YOU),
            "A fine-grained one reaches only the repositories you choose, of one owner, and ends when you say. A \
             classic one reaches everything you can.",
            token,
            cx,
        ),
        choice(Lucide::Bot, "Make an app of your own", Some("Agents act as its bot"), app_words::CHOICE_SAYS, app, cx),
    ];
    let offered = choices(gh.into_iter().chain(ways), cx);
    let body = v_flex().gap(look::Space::Small).child(h_flex().child(which(cx))).child(offered);
    let framed_as = Framed {
        title: "Connect a GitHub account".into(),
        lead: "Who agents are on GitHub when they push, open pull requests and comment: you, or an app of your own."
            .into(),
        body: Some(body.into_any_element()),
        refused: choosing.refused.clone(),
        buttons: buttons().child(cancel(drawing.kept)),
    };
    framed(drawing, panel, framed_as, cx)
}

/// The link to the guide's page on which way to choose.
fn which(cx: &App) -> impl IntoElement {
    h_flex()
        .id("git-which")
        .hand()
        .gap(look::Space::XSmall)
        .text_color(look::colors(cx).link)
        .child("Which should I choose?")
        .child(Icon::new(Lucide::ArrowUpRight).sized(look::IconSize::Regular))
        .on_click(|_, _, cx| cx.open_url(WHICH))
        .test_support()
}

/// The panel a token is pasted into, for a new account or in place of an account's own login: what makes one, the
/// field, which has the cursor and shows only dots, and Connect, or Renew, which is off until there's a token, and
/// while the hub hasn't answered.
pub(super) fn token(drawing: &Drawing<'_>, panel: &Panel, token: &TokenPanel, cx: &App) -> AnyElement {
    let (title, lead, act): (SharedString, SharedString, _) = match &token.renewing {
        None => (
            "Use a token".into(),
            "Make a token on GitHub for the repositories agents should reach, and paste it here.".into(),
            "Connect",
        ),
        Some(renewing) if renewing.held => (
            format!("Renew {}", renewing.name).into(),
            format!("Paste a new token made as {} on GitHub. It takes the old one's place.", renewing.name).into(),
            "Renew",
        ),
        Some(renewing) => (
            format!("Connect {} again", renewing.name).into(),
            format!("Paste a token made as {} on GitHub, so the hub holds a login for it again.", renewing.name).into(),
            "Connect",
        ),
    };
    let make =
        look::button("git-make-token", look::ButtonKind::Outlined, look::ControlSize::Regular, "Make one on GitHub")
            .icon(Lucide::ExternalLink)
            .on_click(|_, _, cx| cx.open_url(MAKE_A_TOKEN));
    let field = labelled("Token", "git-token", &token.token.input, "Only the hub keeps it.", cx);
    let connecting = drawing.kept.clone();
    let connect = button("git-panel-connect", act, true, !drawing.kept.read(cx).can_connect(cx), move |window, cx| {
        connecting.update(cx, |kept, cx| kept.connect(window, cx));
    });
    let framed_as = Framed {
        title,
        lead,
        body: Some(v_flex().gap(look::Space::Large).child(h_flex().child(make)).child(field).into_any_element()),
        refused: token.refused.clone(),
        buttons: buttons().child(cancel(drawing.kept)).child(connect),
    };
    framed(drawing, panel, framed_as, cx)
}

/// Cancel, which puts the panel away, and any wait for the hub's answer with it.
fn cancel(kept: &gpui_kit::Entity<Kept>) -> look::Button {
    let kept = kept.clone();
    button("git-cancel", "Cancel", false, false, move |window: &mut Window, cx| {
        kept.update(cx, |kept, cx| kept.put_away_panel(window, cx));
    })
}
