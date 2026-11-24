//! Added, the GitHub accounts the person gave the hub: Connect account by its heading, the panel for a new account
//! under that, what stops the default being used, as the hub says, if anything does, a row for each account, with its
//! menu, and one for each app of your own installed nowhere yet.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, div};
use look::Typeset as _;
use protocol::model::{GithubAccount, GithubAccountId, SentCredential};

use super::super::account_row::{self, Detail, Picture, Row, Tone};
use super::super::lists::{heading, list as rows_of, said_in_the_list};
use super::git_page::COLUMNS;
use super::kept::{Asked, Kept, Panel, Shows};
use super::words::{self, Standing};
use super::{Drawing, account_menu, app_panel_drawn, app_rows, connect_panels, sign_in_panel};
use crate::status_box::{self, StatusBox};

pub(super) fn list(drawing: &Drawing<'_>, cx: &App) -> Div {
    let kept = drawing.kept.read(cx);
    let at_top = kept.panel.as_ref().filter(|panel| panel.under().is_none());
    let opening = drawing.kept.clone();
    let connect =
        look::button("git-connect", look::ButtonKind::Outlined, look::ControlSize::Regular, "Connect account")
            .icon(IconName::Plus)
            .off(!drawing.can_ask(cx) || at_top.is_some())
            .on_click(move |_, _, cx| opening.update(cx, Kept::open_choosing));
    let panel = at_top.map(|panel| div().py(look::Space::Large).child(draw_panel(drawing, panel, cx)));
    let accounts = listed(drawing.records.github_accounts(), drawing.github.and_then(|github| github.account.as_ref()));
    let said = if drawing.older {
        Some(("git-older-hub", words::OLDER_HUB.to_string()))
    } else {
        (accounts.is_empty() && panel.is_none()).then(|| ("git-none-added", words::none_added()))
    };
    let said = said.map(|(id, said)| said_in_the_list(id, said.into(), cx));
    let rows = accounts.iter().map(|account| account_row(drawing, account, cx));
    let blocked = blocked(drawing, !accounts.is_empty(), cx);
    let list = rows_of(cx).children(panel).children(blocked).children(said).children(rows);
    let list = list.children(app_rows::not_installed(drawing, cx));
    let says = if drawing.older { words::ADDED_SAYS_OLDER.to_string() } else { words::added_says() };
    v_flex().child(heading("Added", None, says, Some(connect.into_any_element()), cx)).child(list)
}

/// The accounts in the order Added lists them: the default first, since it's the one used, then those the hub holds a
/// login for, then those it doesn't, each by username, as client-core lists them.
pub(super) fn listed<'a>(
    mut listed: Vec<&'a GithubAccount>,
    default: Option<&GithubAccountId>,
) -> Vec<&'a GithubAccount> {
    listed.sort_by_key(|account| (Some(&account.id) != default, account.held_login.is_none()));
    listed
}

/// The panel that's open, whichever it is.
pub(super) fn draw_panel(drawing: &Drawing<'_>, panel: &Panel, cx: &App) -> AnyElement {
    match &panel.shows {
        Shows::Choosing(choosing) => connect_panels::choosing(drawing, panel, choosing, cx),
        Shows::Token(token) => connect_panels::token(drawing, panel, token, cx),
        Shows::SignIn(sign_in) => sign_in_panel::sign_in(drawing, panel, sign_in, cx),
        Shows::App(app) => app_panel_drawn::app(drawing, panel, app, cx),
    }
}

/// What stops the default being used, and what to do about it, while the hub blocks every worker: agents can't push
/// until it's put right, rather than push as the machine's own. In the hub's words, as the block gives them to git,
/// and as a workspace's line says them. So is there being no default while there are accounts, as once the default was
/// disconnected, whose rows `has_accounts` says there are. With none, Added says to connect one in place of its rows.
fn blocked(drawing: &Drawing<'_>, has_accounts: bool, cx: &App) -> Option<AnyElement> {
    let (lead, reason) = match drawing.sent()? {
        SentCredential::Blocked { reason, .. } => ("Roundhouse can't use its GitHub account.", reason),
        SentCredential::NoAccount { reason } if has_accounts => ("No account is the default.", reason),
        _ => return None,
    };
    let said = StatusBox {
        id: "git-blocked",
        icon: Lucide::TriangleAlert,
        tone: status_box::Tone::Problem,
        lead: Some(lead.into()),
        detail: reason.clone().into(),
        progress: None,
        action: None,
    };
    Some(div().py(look::Space::Large).child(said.draw(cx)).into_any_element())
}

/// `account`'s row, with why the hub couldn't do what was asked of it under it, and the panel renewing its login.
fn account_row(drawing: &Drawing<'_>, account: &GithubAccount, cx: &App) -> AnyElement {
    let standing = drawing.standing(account, cx);
    let tone = match standing {
        Standing::Works { .. } | Standing::Disconnected => Tone::Plain,
        Standing::Renewing | Standing::GhUnread => Tone::Warning,
        Standing::Ended { .. }
        | Standing::SignInAgain
        | Standing::CantBeUsed
        | Standing::GhLoggedOut
        | Standing::GhOtherUser => Tone::Problem,
    };
    let kept = drawing.kept.read(cx);
    let failed = kept.failed(&Asked::Account(account.id.clone())).map(|said| failed_line(said, cx).into_any_element());
    let panel = kept.panel.as_ref().filter(|panel| panel.under() == Some(&account.id));
    let panel =
        panel.map(|panel| div().pb(look::Space::Large).child(draw_panel(drawing, panel, cx)).into_any_element());
    let (picture, detail, menu) = match &account.installation {
        Some(installed) => app_rows::installation_parts(drawing, account, installed, &standing, tone, cx),
        None => (
            Picture::of(&drawing.records.github_username(account).to_string()),
            standing.said().map(|said| Detail::Said(said, tone)),
            Some(account_menu::menu(drawing, account, standing.works(), cx)),
        ),
    };
    let shown = Row {
        id: SharedString::from(format!("git-account-{}", account.id)),
        picture,
        name: drawing.records.github_username(account).to_string(),
        dim: standing == Standing::Disconnected,
        default: drawing.is_default(account),
        how: account.held_login.map(|held| words::connected_by(held.how)),
        detail,
        menu,
        card: None,
        under: failed.into_iter().chain(panel).collect(),
    };
    account_row::row(shown, COLUMNS, cx)
}

/// Why the hub couldn't do what was asked of a row, or of Found, in red, under it.
pub(super) fn failed_line(said: &str, cx: &App) -> Div {
    let line = div().pb(look::Space::Medium).typeset(look::Type::Body).text_color(look::colors(cx).error);
    line.child(said.to_string())
}
