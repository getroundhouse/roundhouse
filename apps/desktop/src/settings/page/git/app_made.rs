//! The panel's steps once GitHub has sent the person back: the hub making the app from GitHub's code, and installing
//! it, which the panel waits for by asking the hub to look as the window comes to the front again, and whenever the
//! person asks. Where it's installed is the hub's record of each installation's account, which the panel reads.

use client_core::records::Snapshot;
use gpui_kit::{Context, Window};
use protocol::client::github_apps::{GithubAppInstallationsConnect, GithubAppMake};
use protocol::model::{GithubAccount, GithubAppId, GithubAppSlug};

use super::app_panel::{AppStep, OnGithub};
use super::app_words;
use super::kept::Kept;
use crate::github_app_pages::install_page;

/// The account that's `app`'s installation, holding a login, once the hub has connected one.
pub(super) fn installation(records: &Snapshot, app: GithubAppId) -> Option<&GithubAccount> {
    let installed = |account: &&GithubAccount| account.installation.as_ref().is_some_and(|on| on.app == app);
    records.github_accounts().into_iter().filter(installed).find(|account| account.held_login.is_some())
}

impl Kept {
    /// Gives the hub the code GitHub sent the person back with, again after it couldn't take it, for it to make the
    /// app, then goes on to installing it.
    pub(super) fn make(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else { return };
        let Some(app) = self.app_panel_mut() else { return };
        let AppStep::OnGithub { stands: OnGithub::Making { code, failed }, .. } = &mut app.step else { return };
        *failed = None;
        let code = code.clone();
        let making = asking.hub.ask(GithubAppMake { code });
        app.waiting = Some(cx.spawn_in(window, async move |this, cx| {
            let answer = making.recv().await;
            // In the window it was asked in, which `update_in` would take to be whichever the page last used.
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| match answer {
                    Ok(made) => this.installing(made.id, window, cx),
                    Err(e) => this.not_made(&e, cx),
                })
            });
        }));
        cx.notify();
    }

    fn not_made(&mut self, e: &client_core::ClientError, cx: &mut Context<Self>) {
        let Some(AppStep::OnGithub { stands: OnGithub::Making { failed, .. }, .. }) =
            self.app_panel_mut().map(|app| &mut app.step)
        else {
            return;
        };
        let (why, again) = app_words::not_made(e);
        *failed = Some(super::app_panel::NotMade { why, again });
        cx.notify();
    }

    /// Has the panel wait for `app` to be installed, asking the hub to look as the window comes to the front again.
    pub(super) fn installing(&mut self, app: GithubAppId, window: &mut Window, cx: &mut Context<Self>) {
        let on_focus = cx.observe_window_activation(window, |kept, window, cx| {
            if window.is_window_active() {
                kept.look_for_installations(cx);
            }
        });
        let Some(panel) = self.app_panel_mut() else { return };
        panel.step = AppStep::Installing { app, looking: false, failed: None };
        (panel.waiting, panel.on_focus) = (None, Some(on_focus));
        cx.notify();
    }

    /// Opens GitHub's page for installing the app `app`, which GitHub calls `slug`, where that's known, and the panel
    /// at its last step, waiting for it, in place of any other.
    pub(super) fn install_on_github(
        &mut self,
        app: GithubAppId,
        slug: Option<&GithubAppSlug>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.open_url(&install_page(slug));
        let installing = matches!(
            self.app_panel().map(|panel| &panel.step),
            Some(AppStep::Installing { app: shown, .. }) if *shown == app
        );
        if !installing {
            self.open_app_at(AppStep::Installing { app, looking: false, failed: None }, cx);
            self.installing(app, window, cx);
        }
    }

    /// Has the hub look for where the app the panel installs is installed now, unless it's looking, or the app is
    /// installed already, and says why it couldn't, if it couldn't.
    pub(super) fn look_for_installations(&mut self, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else { return };
        let Some(panel) = self.app_panel_mut() else { return };
        let AppStep::Installing { app, looking, failed } = &mut panel.step else { return };
        if *looking || installation(&asking.feed.snapshot(), *app).is_some() {
            return;
        }
        (*looking, *failed) = (true, None);
        let id = *app;
        let looking = asking.hub.ask(GithubAppInstallationsConnect { id });
        panel.waiting = Some(cx.spawn(async move |this, cx| {
            let answer = looking.recv().await;
            let _ = this.update(cx, |this, cx| {
                let said = answer.err().map(|e| app_words::not_looked(&e));
                if let Some(AppStep::Installing { looking, failed, .. }) = this.app_panel_mut().map(|app| &mut app.step)
                {
                    (*looking, *failed) = (false, said);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
