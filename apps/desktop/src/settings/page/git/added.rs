//! Added, the GitHub accounts the person gave the hub: Connect account by its heading, the panel for a new account
//! under that, what stops the default being used, if anything does, and a row for each account, with its menu.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::Button;
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, div};
use look::{Off as _, Sizing as _, Typeset as _};
use protocol::model::{GithubAccount, GithubAccountId};

use super::super::account_row::{self, Detail, Picture, Row, Tone};
use super::super::lists::{heading, list as rows_of, said_in_the_list};
use super::git_page::COLUMNS;
use super::kept::{Asked, Panel, Shows};
use super::words::{self, Standing};
use super::{Drawing, account_menu, connect_panels, sign_in_panel};
use crate::status_box::{self, StatusBox};

pub(super) fn list(drawing: &Drawing<'_>, cx: &App) -> Div {
    let kept = drawing.kept.read(cx);
    let at_top = kept.panel.as_ref().filter(|panel| panel.under().is_none());
    let opening = drawing.kept.clone();
    let connect = Button::new("git-connect")
        .outline()
        .sized(look::ControlSize::Regular)
        .icon(IconName::Plus)
        .label("Connect account")
        .off_while(!drawing.can_ask(cx) || at_top.is_some())
        .on_click(move |_, _, cx| opening.update(cx, |kept, cx| kept.open(Shows::Choosing, cx)));
    let panel = at_top.map(|panel| div().py(look::Space::Large).child(draw_panel(drawing, panel, cx)));
    let accounts = listed(drawing.records.github_accounts(), drawing.github.and_then(|github| github.account.as_ref()));
    let said = if drawing.older {
        Some(("git-older-hub", words::OLDER_HUB))
    } else {
        (accounts.is_empty() && panel.is_none()).then_some(("git-none-added", words::NONE_ADDED))
    };
    let said = said.map(|(id, said)| said_in_the_list(id, said.into(), cx));
    let rows = accounts.iter().map(|account| account_row(drawing, account, cx));
    let list = rows_of(cx).children(panel).children(blocked(drawing, cx)).children(said).children(rows);
    let says = if drawing.older { words::ADDED_SAYS_OLDER } else { words::ADDED_SAYS };
    v_flex().child(heading("Added", None, says, Some(connect.into_any_element()), cx)).child(list)
}

/// The accounts in the order Added lists them: the default first, since it's the one used, then those the hub holds a
/// login for, then those it doesn't, each by username, as client-core lists them.
pub(super) fn listed<'a>(accounts: &'a [GithubAccount], default: Option<&GithubAccountId>) -> Vec<&'a GithubAccount> {
    let mut listed: Vec<&GithubAccount> = accounts.iter().collect();
    listed.sort_by_key(|account| (Some(&account.id) != default, account.held_login.is_none()));
    listed
}

/// The panel that's open, whichever it is.
pub(super) fn draw_panel(drawing: &Drawing<'_>, panel: &Panel, cx: &App) -> AnyElement {
    match &panel.shows {
        Shows::Choosing => connect_panels::choosing(drawing, panel, cx),
        Shows::Token(token) => connect_panels::token(drawing, panel, token, cx),
        Shows::SignIn(sign_in) => sign_in_panel::sign_in(drawing, panel, sign_in, cx),
    }
}

/// What stops the default being used, and what to do about it, when it can't be: agents can't push until it's put
/// right, rather than push as the machine's own.
fn blocked(drawing: &Drawing<'_>, cx: &App) -> Option<AnyElement> {
    let account = drawing.records.github_account(drawing.github?.account.as_ref()?)?;
    let said = Standing::of(account.held_login.as_ref(), drawing.now_ms, cx).blocked(account.user.name.as_str())?;
    let said = StatusBox {
        id: "git-blocked",
        icon: Lucide::TriangleAlert,
        tone: status_box::Tone::Problem,
        lead: Some("Agents can't push.".into()),
        detail: said.into(),
        progress: None,
        action: None,
    };
    Some(div().py(look::Space::Large).child(said.draw(cx)).into_any_element())
}

/// `account`'s row, with why the hub couldn't do what was asked of it under it, and the panel renewing its login.
fn account_row(drawing: &Drawing<'_>, account: &GithubAccount, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let standing = Standing::of(account.held_login.as_ref(), drawing.now_ms, cx);
    let tone = match standing {
        Standing::Works { .. } | Standing::Disconnected => Tone::Plain,
        Standing::Renewing => Tone::Warning,
        Standing::Ended { .. } | Standing::SignInAgain => Tone::Problem,
    };
    let kept = drawing.kept.read(cx);
    let failed = kept.failed(&Asked::Account(account.id.clone())).map(|said| {
        let line = div().pb(look::Space::Medium).typeset(look::Type::Body).text_color(colors.error);
        line.child(said.to_string()).into_any_element()
    });
    let panel = kept.panel.as_ref().filter(|panel| panel.under() == Some(&account.id));
    let panel =
        panel.map(|panel| div().pb(look::Space::Large).child(draw_panel(drawing, panel, cx)).into_any_element());
    let shown = Row {
        id: SharedString::from(format!("git-account-{}", account.id)),
        picture: Picture::of(account.user.name.as_str()),
        name: account.user.name.to_string(),
        dim: standing == Standing::Disconnected,
        default: drawing.is_default(account),
        how: account.held_login.map(|held| words::connected_by(held.how).into()),
        detail: standing.said().map(|said| Detail::Said(said, tone)),
        menu: Some(account_menu::menu(drawing, account, standing.works(), cx)),
        card: None,
        under: failed.into_iter().chain(panel).collect(),
    };
    account_row::row(shown, COLUMNS, cx)
}
