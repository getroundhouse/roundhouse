//! The `…` menus of an app's rows. An installation's has Make default, but on the default, which is off while its login
//! can't be used; Disconnect while the hub holds its login, and otherwise Connect again and Install on GitHub; and
//! Remove app while the hub has the app. An app installed nowhere has Install on GitHub, Check again and Remove app.
//! Each is off with no hub to ask.

use std::rc::Rc;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{AnyElement, App, Context, Entity, SharedString, Window};
use protocol::model::{GithubAccount, GithubApp, GithubInstallation};

use super::super::account_row;
use super::Drawing;
use super::app_words;
use super::kept::{Asked, Kept};

/// What an item does to the page's state.
type Act = Rc<dyn Fn(&mut Kept, &mut Window, &mut Context<Kept>)>;

/// An item, made into the kit's each time the menu opens.
struct Item {
    label: &'static str,
    off: bool,
    act: Act,
}

/// The menu of `app`, installed nowhere.
pub(super) fn app_menu(drawing: &Drawing<'_>, app: &GithubApp, cx: &App) -> AnyElement {
    let at = Asked::App(app.id);
    let items = vec![install(app), check_again("Check again", app, at.clone()), remove(app, false, at)];
    menu_of(
        drawing.kept,
        SharedString::from(format!("git-more-app-{}", app.id)),
        &app.name,
        !drawing.can_ask(cx),
        items,
    )
}

/// The menu of `account`, an installation as `installed` says, whose login works when `works`. `None` when it would
/// have nothing in it, as for the default once its app has been removed.
pub(super) fn installation_menu(
    drawing: &Drawing<'_>,
    account: &GithubAccount,
    installed: &GithubInstallation,
    works: bool,
    cx: &App,
) -> Option<AnyElement> {
    let default = drawing.is_default(account);
    let app = drawing.records.github_app(installed.app);
    let at = Asked::Account(account.id.clone());
    let mut items = Vec::new();
    if !default {
        let id = account.id.clone();
        let act: Act = Rc::new(move |kept, _, cx| kept.make_default(Some(id.clone()), cx));
        items.push(Item { label: "Make default", off: !works, act });
    }
    if account.held_login.is_some() {
        let (id, name) = (account.id.clone(), account.user.name.to_string());
        let says = app_words::disconnect_says(default);
        let act: Act = Rc::new(move |kept, window, cx| kept.ask_to_disconnect(id.clone(), &name, says, window, cx));
        items.push(Item { label: "Disconnect", off: false, act });
    } else if let Some(app) = app {
        items.extend([check_again("Connect again", app, at.clone()), install(app)]);
    }
    items.extend(app.map(|app| remove(app, default, at)));
    let id = SharedString::from(format!("git-more-{}", account.id));
    let off = !drawing.can_ask(cx);
    (!items.is_empty()).then(|| menu_of(drawing.kept, id, account.user.name.as_str(), off, items))
}

/// Install on GitHub, which opens GitHub's page for installing `app`, and the panel waiting for it.
fn install(app: &GithubApp) -> Item {
    let app = app.clone();
    Item {
        label: "Install on GitHub",
        off: false,
        act: Rc::new(move |kept, window, cx| kept.install_on_github(&app, window, cx)),
    }
}

/// Has the hub look for where `app` is installed, and connect it there, saying `at` why it couldn't.
fn check_again(label: &'static str, app: &GithubApp, at: Asked) -> Item {
    let id = app.id;
    Item { label, off: false, act: Rc::new(move |kept, _, cx| kept.connect_installations(id, at.clone(), cx)) }
}

/// Remove app, for `app`, whose installation is the default when `default`, saying `at` why the hub couldn't.
fn remove(app: &GithubApp, default: bool, at: Asked) -> Item {
    let app = app.clone();
    let act: Act = Rc::new(move |kept, window, cx| kept.ask_to_remove_app(&app, default, at.clone(), window, cx));
    Item { label: "Remove app", off: false, act }
}

/// A row's `…` menu of `items`, for what's called `name`, known to tests by `id`, off while `off`.
fn menu_of(kept: &Entity<Kept>, id: SharedString, name: &str, off: bool, items: Vec<Item>) -> AnyElement {
    let kept = kept.clone();
    account_row::menu(id, name, off, move |menu: PopupMenu, _, _| {
        items.iter().fold(menu, |menu, item| {
            let (kept, act) = (kept.clone(), Rc::clone(&item.act));
            let made = PopupMenuItem::new(item.label).disabled(item.off);
            menu.item(made.on_click(move |_, window, cx| kept.update(cx, |kept, cx| act(kept, window, cx))))
        })
    })
}
