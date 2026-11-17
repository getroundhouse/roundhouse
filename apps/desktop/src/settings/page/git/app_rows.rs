//! The rows of apps of your own in Added: each installation's, as an account whose user is the app's bot, with the
//! app's mark, `App`, and where it's installed, and a row for each app installed nowhere yet, which says so, and what
//! stops a default that's an installation being used.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::{AnyElement, App, IntoElement as _, SharedString};
use protocol::model::{GithubAccount, GithubApp, GithubInstallation};

use super::super::account_row::{self, Detail, Picture, Row, Tone};
use super::Drawing;
use super::added::failed_line;
use super::app_menu::{app_menu, installation_menu};
use super::app_words;
use super::git_page::COLUMNS;
use super::kept::Asked;
use super::words::Standing;

/// What an installation's row shows that an account's doesn't: the app's mark, where it's installed while its login
/// works, or else how it stands, in `tone`, and its menu, if it has anything in it.
pub(super) fn installation_parts(
    drawing: &Drawing<'_>,
    account: &GithubAccount,
    installed: &GithubInstallation,
    standing: &Standing,
    tone: Tone,
    cx: &App,
) -> (Picture, Option<Detail>, Option<AnyElement>) {
    let detail = match standing.said() {
        Some(said) if !standing.works() => Detail::Said(said, tone),
        Some(_) | None => Detail::Said(app_words::installed_on(installed.on.name.as_str()), Tone::Plain),
    };
    (Picture::Mark(Lucide::Bot), Some(detail), installation_menu(drawing, account, installed, standing.works(), cx))
}

/// What stops the default, the installation `account`, being used, as its login stands, and what to do, if it can't.
pub(super) fn blocked(
    drawing: &Drawing<'_>,
    account: &GithubAccount,
    installed: &GithubInstallation,
    standing: &Standing,
) -> Option<String> {
    let renewing = match standing {
        Standing::Works { .. } => return None,
        Standing::Renewing | Standing::Ended { .. } => true,
        Standing::SignInAgain | Standing::Disconnected => false,
    };
    let app_held = drawing.records.github_app(installed.app).is_some();
    Some(app_words::blocked(account.user.name.as_str(), renewing, app_held))
}

/// A row for each app of your own that no account is an installation of, saying it's installed nowhere.
pub(super) fn not_installed(drawing: &Drawing<'_>, cx: &App) -> Vec<AnyElement> {
    let accounts = drawing.records.github_accounts();
    let installed = |app: &GithubApp| {
        accounts.iter().any(|account| account.installation.as_ref().is_some_and(|installed| installed.app == app.id))
    };
    drawing.records.github_apps().iter().filter(|app| !installed(app)).map(|app| app_row(drawing, app, cx)).collect()
}

/// `app`'s row, with why the hub couldn't do what was asked of it under it.
fn app_row(drawing: &Drawing<'_>, app: &GithubApp, cx: &App) -> AnyElement {
    let failed = drawing.kept.read(cx).failed(&Asked::App(app.id)).map(|said| failed_line(said, cx).into_any_element());
    let shown = Row {
        id: SharedString::from(format!("git-app-{}", app.id)),
        picture: Picture::Mark(Lucide::Bot),
        name: app.name.clone(),
        dim: false,
        default: false,
        how: Some(app_words::APP.into()),
        detail: Some(Detail::Said(app_words::NOT_INSTALLED.to_string(), Tone::Warning)),
        menu: Some(app_menu(drawing, app, cx)),
        card: None,
        under: failed.into_iter().collect(),
    };
    account_row::row(shown, COLUMNS, cx)
}
