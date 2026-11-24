//! What an app's rows ask the hub, outside the panel: to look for where an app is installed, and connect it there
//! again, and to remove an app, which asks first. Why the hub couldn't is said under the row it was asked from.

use gpui_kit::{Context, Window};
use protocol::model::GithubAppId;

use super::kept::{Asked, Kept};
use super::{app_words, words};
use crate::asking::{self, Question};

impl Kept {
    /// Has the hub look for where `app` is installed now, and connect it there, as when it was disconnected, saying
    /// `at` why it couldn't, if it couldn't.
    pub(super) fn connect_installations(&mut self, app: GithubAppId, at: Asked, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else {
            self.failed = Some((at, words::NO_HUB.to_string()));
            cx.notify();
            return;
        };
        self.failed = None;
        let looking = asking.client.start(move |client| client.github_app_installations_connect(app));
        cx.spawn(async move |this, cx| {
            let answer = looking.recv().await;
            let _ = this.update(cx, |this, cx| {
                if let Err(e) = answer {
                    this.failed = Some((at, app_words::not_looked(&e)));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Asks whether to have the hub forget the app `app`, called `name`, whose installation is the default when
    /// `default`, and does once it's confirmed, saying `at` why it couldn't, if it couldn't.
    pub(super) fn ask_to_remove_app(
        &mut self,
        app: GithubAppId,
        name: &str,
        default: bool,
        at: Asked,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.failed = None;
        let asked = Question {
            id: "remove-github-app",
            title: app_words::remove_asks(name).into(),
            says: app_words::remove_says(default).into(),
            keep: "Cancel",
            act: "Remove",
            removes: true,
        };
        let (id, this) = (app, cx.weak_entity());
        asking::ask(asked, window, cx, move |_, cx| {
            let _ = this.update(cx, |this, cx| this.remove_app(id, at.clone(), cx));
        });
    }

    /// Has the hub forget the app `id`. Its answer is the app as it last was, and the feed says it's been removed.
    fn remove_app(&mut self, id: GithubAppId, at: Asked, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else {
            self.failed = Some((at, words::NO_HUB.to_string()));
            cx.notify();
            return;
        };
        let removing = asking.client.start(move |client| client.github_app_remove(id));
        cx.spawn(async move |this, cx| {
            if let Err(e) = removing.recv().await {
                let _ = this.update(cx, |this, cx| {
                    this.failed = Some((at, app_words::couldnt("remove it", &e)));
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
