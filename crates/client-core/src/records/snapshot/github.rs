//! What a hub says of GitHub, as views read it: the GitHub accounts it holds, the sign-ins with GitHub it has started,
//! and the apps of your own it holds the key of, each as the hub's records have it, with what GitHub says of each app,
//! and what a push from each repository would meet, as the hub's observations have it.

use std::collections::HashMap;

use protocol::model::{
    GithubAccount, GithubAccountId, GithubApp, GithubAppId, GithubAppSaid, GithubSignIn, GithubSignInId, GithubUser,
    Observation, ObservationKey, Reach, RepoId,
};

use super::observed::said;
use super::{GithubName, Snapshot};

#[derive(Debug, Default)]
pub(super) struct GithubListed {
    /// In no order, since they're shown by username, which a view reads apart from the record, as
    /// [`Snapshot::github_username`] says.
    accounts: HashMap<GithubAccountId, GithubAccount>,
    sign_ins: HashMap<GithubSignInId, GithubSignIn>,
    /// In no order, since they're shown by name, which a view reads apart from the record, as
    /// [`Snapshot::github_app_said`] says.
    apps: HashMap<GithubAppId, GithubApp>,
}

impl GithubListed {
    pub(super) fn account(&mut self, account: &GithubAccount) {
        self.accounts.insert(account.id.clone(), account.clone());
    }

    pub(super) fn sign_in(&mut self, sign_in: &GithubSignIn) {
        self.sign_ins.insert(sign_in.id.clone(), sign_in.clone());
    }

    pub(super) fn app(&mut self, app: &GithubApp) {
        self.apps.insert(app.id, app.clone());
    }
}

impl Snapshot {
    /// Every GitHub account the hub holds, or held, a login for, by username, in any case, then by ID, since one GitHub
    /// user can be connected more than once. One whose username isn't known comes after those named, by its user's
    /// number. Empty until they've arrived, and from a hub that keeps none, which its settings tell apart, as
    /// [`HubSettings::github`](protocol::model::HubSettings::github) says.
    pub fn github_accounts(&self) -> Vec<&GithubAccount> {
        let mut accounts: Vec<&GithubAccount> = self.records.github.accounts.values().collect();
        accounts.sort_by_cached_key(|account| {
            let name = match self.github_username(account) {
                GithubName::Named(name) => Ok(name.as_str().to_lowercase()),
                GithubName::NotKnown(number) => Err(number.get()),
            };
            (name, account.id.clone())
        });
        accounts
    }

    pub fn github_account(&self, id: &GithubAccountId) -> Option<&GithubAccount> {
        self.records.github.accounts.get(id)
    }

    /// The record of the sign-in with GitHub `id`, as long as the hub keeps it: while it waits, and once it has ended,
    /// until another starts.
    pub fn github_sign_in(&self, id: &GithubSignInId) -> Option<&GithubSignIn> {
        self.records.github.sign_ins.get(id)
    }

    /// Every sign-in with GitHub the hub keeps a record of, in no order.
    pub fn github_sign_ins(&self) -> impl Iterator<Item = &GithubSignIn> {
        self.records.github.sign_ins.values()
    }

    /// Every app of your own the hub holds the key of, by name, in any case, then by number. One whose name isn't known
    /// comes after those named, by its number.
    pub fn github_apps(&self) -> Vec<&GithubApp> {
        let mut apps: Vec<&GithubApp> = self.records.github.apps.values().collect();
        apps.sort_by_cached_key(|app| {
            (self.github_app_said(app).map(|said| said.name.to_lowercase()).ok_or(()), app.id.get())
        });
        apps
    }

    /// The record of the app of your own `id`, until it's removed.
    pub fn github_app(&self, id: GithubAppId) -> Option<&GithubApp> {
        self.records.github.apps.get(&id)
    }

    /// What GitHub says of `app`: its slug, its name and its owner, as the hub last asked GitHub as the app. `None`
    /// while that isn't known, as for an app whose key is gone, which is drawn by its number.
    pub fn github_app_said<'a>(&'a self, app: &'a GithubApp) -> Option<&'a GithubAppSaid> {
        let held = self.observation(&ObservationKey::GithubAppSaid { app: app.id });
        let Some(Observation::GithubAppSaid { said: app_said, .. }) = held else { return None };
        said(app_said)
    }

    /// What `app` is called, as GitHub says, or its number, as `#7`, while that isn't known.
    pub fn github_app_name(&self, app: &GithubApp) -> String {
        self.github_app_said(app).map_or_else(|| format!("#{}", app.id), |said| said.name.clone())
    }

    /// `app`'s bot, which commits made through it are by, as the hub last asked GitHub. `None` until it has.
    pub fn github_app_bot<'a>(&'a self, app: &'a GithubApp) -> Option<&'a GithubUser> {
        let Some(Observation::GithubAppBot { bot, .. }) =
            self.observation(&ObservationKey::GithubAppBot { app: app.id })
        else {
            return None;
        };
        said(bot)
    }

    /// What a push from the repository `id`'s workspaces would meet, as the hub last checked, or `None` while it hasn't,
    /// and from a hub that doesn't check.
    pub fn github_reach(&self, id: &RepoId) -> Option<&Reach> {
        let Some(Observation::RepositoryReach { reach, .. }) =
            self.observation(&ObservationKey::RepositoryReach { repo: id.clone() })
        else {
            return None;
        };
        said(reach)
    }
}
