//! Found, the `gh` login each provider's machines already have, as the hub found it once it started: who it's logged in
//! as, or that it's logged in to no one or isn't installed, and whether GitHub took its token. Roundhouse never copies
//! one, so a row has no menu. Each machine's own login is a choice of default too, the one a hub with no account has:
//! Found's heading says Default while it is, and offers Make default while it isn't. *Unconfirmed:* choosing it there.

use gpui_kit::component::button::Button;
use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, div};
use look::{Off as _, Sizing as _, Typeset as _};
use protocol::model::{FoundGithubLogin, GhLogin};

use super::super::account_row::{self, Detail, Picture, Row, Tone};
use super::super::lists::{badge, heading, list as rows_of, said_in_the_list};
use super::git_page::COLUMNS;
use super::kept::Asked;
use super::{Drawing, words};

/// Found, but for a hub that keeps no GitHub accounts, whose Added says it can't, and which finds no logins either.
pub(super) fn list(drawing: &Drawing<'_>, cx: &App) -> Option<Div> {
    if drawing.older {
        return None;
    }
    let machines_own = drawing.github.is_some_and(|github| github.account.is_none());
    let found = drawing.records.found_github_logins();
    let none = found.is_empty().then(|| said_in_the_list("git-none-found", words::NONE_FOUND.into(), cx));
    let failed = drawing.kept.read(cx).failed(&Asked::MachinesOwn).map(|said| {
        let line = div().pb(look::Space::Medium).typeset(look::Type::Body).text_color(look::colors(cx).error);
        line.child(said.to_string())
    });
    let rows = found.iter().map(|found| found_row(found, cx));
    let heading =
        heading("Found", machines_own.then(|| badge("Default", cx)), words::FOUND_SAYS, make_default(drawing, cx), cx);
    Some(v_flex().child(heading).children(failed).child(rows_of(cx).children(none).children(rows)))
}

/// Make default, which makes each machine's own login the default, while an account the hub holds is, and nothing
/// while each machine's own is, or while the hub's settings haven't said which is. It's off with no hub to ask, and
/// while the hub hasn't answered.
fn make_default(drawing: &Drawing<'_>, cx: &App) -> Option<AnyElement> {
    drawing.github?.account.as_ref()?;
    let kept = drawing.kept.clone();
    let button = Button::new("git-machines-own-default")
        .accessibility_label("Make each machine's own login the default")
        .outline()
        .sized(look::ControlSize::Regular)
        .label("Make default")
        .off_while(!drawing.can_ask(cx) || drawing.kept.read(cx).choosing())
        .on_click(move |_, _, cx| kept.update(cx, |kept, cx| kept.make_default(None, cx)));
    Some(button.into_any_element())
}

/// A provider's login's row: who `gh` there is logged in as, whose `gh` it is, and whether it works.
fn found_row(found: &FoundGithubLogin, cx: &App) -> AnyElement {
    let (name, said) = words::found(&found.login);
    let someone = matches!(found.login, GhLogin::LoggedIn { .. });
    let shown = Row {
        id: SharedString::from(format!("git-found-{}", found.provider)),
        picture: if someone { Picture::of(&name) } else { Picture::Nobody },
        dim: !someone,
        name,
        default: false,
        how: Some(words::found_on(&found.provider).into()),
        detail: said
            .map(|(said, problem)| Detail::Said(said.to_string(), if problem { Tone::Problem } else { Tone::Plain })),
        menu: None,
        card: None,
        under: Vec::new(),
    };
    account_row::row(shown, COLUMNS, cx)
}
