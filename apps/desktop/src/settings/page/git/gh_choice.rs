//! Connect account's panel, and the way of connecting an account it offers only once the hub has said `gh` on its
//! machine is logged in: using that login, whose token the hub reads from `gh` and keeps no copy of. The hub is asked
//! who `gh` is each time the panel opens, since `gh` can be logged in or out at any time.

use client_core::records::Snapshot;
use gpui_kit::{Context, Task, Window};
use protocol::client::github_accounts::{GithubAccountConnectGh, HubGhLogin};
use protocol::model::{GhLogin, GhLoginWorks, GithubLoginKind, GithubUsername};

use super::kept::{Kept, Shows};
use super::words;

/// Connect account's panel, choosing how to connect an account.
#[derive(Default)]
pub(super) struct ChoosingPanel {
    /// Who `gh` on the hub's machine is logged in as, once the hub has said, and `None` until then, or when the hub
    /// couldn't say.
    pub(super) gh: Option<GhLogin>,
    /// What the hub said when it last refused to use `gh`'s login, for the person to read.
    pub(super) refused: Option<String>,
    /// The wait for who `gh` is, while the hub hasn't said. It goes with the panel, as does the next.
    reading: Option<Task<()>>,
    /// The wait for the hub to use `gh`'s login, while it hasn't answered.
    connecting: Option<Task<()>>,
}

impl ChoosingPanel {
    /// Who to offer `gh`'s login as, by its username: only one GitHub takes, and that none of the GitHub accounts of
    /// `records` holds as `gh`'s already. None while any account's username isn't known, since it can't tell.
    pub(super) fn offered<'a>(&'a self, records: &Snapshot) -> Option<&'a GithubUsername> {
        let Some(GhLogin::LoggedIn { username, works }) = &self.gh else { return None };
        let mut held = false;
        for account in records.github_accounts() {
            let name = records.github_username(account).known()?;
            held |= account.held_login.is_some_and(|held| held.how == GithubLoginKind::Gh)
                && name.as_str().eq_ignore_ascii_case(username.as_str());
        }
        (*works != GhLoginWorks::Refused && !held).then_some(username)
    }

    /// Whether the panel waits for the hub to use `gh`'s login.
    pub(super) fn connecting(&self) -> bool {
        self.connecting.is_some()
    }
}

impl Kept {
    /// Opens Connect account's panel, and asks the hub who `gh` on its machine is, to offer its login once it says.
    pub(super) fn open_choosing(&mut self, cx: &mut Context<Self>) {
        let mut panel = ChoosingPanel::default();
        if let Some(asking) = self.asking.clone() {
            let reading = asking.hub.ask(HubGhLogin);
            panel.reading = Some(cx.spawn(async move |this, cx| {
                let answer = reading.recv().await;
                let _ = this.update(cx, |this, cx| {
                    let Some(panel) = this.choosing_panel_mut() else { return };
                    (panel.gh, panel.reading) = (answer.ok(), None);
                    cx.notify();
                });
            }));
        }
        self.open(Shows::Choosing(panel), cx);
    }

    /// Has the hub use `gh`'s login on its machine as an account's, and closes the panel once it has, or says why it
    /// couldn't.
    pub(super) fn use_gh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else { return };
        let Some(panel) = self.choosing_panel_mut().filter(|panel| !panel.connecting()) else { return };
        let connecting = asking.hub.ask(GithubAccountConnectGh);
        panel.refused = None;
        panel.connecting = Some(cx.spawn_in(window, async move |this, cx| {
            let answer = connecting.recv().await;
            let _ = this.update_in(cx, |this, window, cx| match answer {
                Ok(_) => this.put_away_panel(window, cx),
                Err(e) => {
                    let Some(panel) = this.choosing_panel_mut() else { return };
                    (panel.refused, panel.connecting) = (Some(words::couldnt("use gh's login", &e)), None);
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    fn choosing_panel_mut(&mut self) -> Option<&mut ChoosingPanel> {
        let Some(Shows::Choosing(panel)) = self.panel.as_mut().map(|panel| &mut panel.shows) else { return None };
        Some(panel)
    }
}
