//! What a hub says of GitHub, as views read it: the GitHub accounts it holds, the sign-ins with GitHub it has started,
//! the apps of your own it holds the key of, the login found on each provider's machines, and what a push from each
//! repository would meet, each as the hub's records have it.

use std::collections::HashMap;

use protocol::model::{
    FoundGithubLogin, GithubAccount, GithubAccountId, GithubApp, GithubAppId, GithubReach, GithubSignIn,
    GithubSignInId, Reach, RepoId,
};

use super::Snapshot;

#[derive(Debug, Default)]
pub(super) struct GithubListed {
    /// By username, in any case, then by ID, since one GitHub user can be connected more than once.
    accounts: Vec<GithubAccount>,
    account_at: HashMap<GithubAccountId, usize>,
    sign_ins: HashMap<GithubSignInId, GithubSignIn>,
    /// By name, in any case, then by number.
    apps: Vec<GithubApp>,
    app_at: HashMap<GithubAppId, usize>,
    /// By provider.
    found: Vec<FoundGithubLogin>,
    /// By the repository it's of.
    reach: HashMap<RepoId, Reach>,
}

impl GithubListed {
    pub(super) fn account(&mut self, account: &GithubAccount) {
        self.accounts.push(account.clone());
    }

    pub(super) fn sign_in(&mut self, sign_in: &GithubSignIn) {
        self.sign_ins.insert(sign_in.id.clone(), sign_in.clone());
    }

    pub(super) fn app(&mut self, app: &GithubApp) {
        self.apps.push(app.clone());
    }

    pub(super) fn found(&mut self, found: &FoundGithubLogin) {
        self.found.push(found.clone());
    }

    pub(super) fn reach(&mut self, reach: &GithubReach) {
        self.reach.insert(reach.repo.clone(), reach.reach.clone());
    }

    /// Puts each list in the order views show it, once every record is in.
    pub(super) fn order(&mut self) {
        self.accounts.sort_by_cached_key(|account| (account.user.name.as_str().to_lowercase(), account.id.clone()));
        self.account_at = self.accounts.iter().enumerate().map(|(at, account)| (account.id.clone(), at)).collect();
        self.apps.sort_by_cached_key(|app| (app.name.to_lowercase(), app.id.get()));
        self.app_at = self.apps.iter().enumerate().map(|(at, app)| (app.id, at)).collect();
        self.found.sort_by(|a, b| (!a.provider.is_local(), &a.provider).cmp(&(!b.provider.is_local(), &b.provider)));
    }
}

impl Snapshot {
    /// Every GitHub account the hub holds, or held, a login for, by username, in any case. Empty until they've arrived,
    /// and from a hub that keeps none, which its settings tell apart, as
    /// [`HubSettings::github`](protocol::model::HubSettings::github) says.
    pub fn github_accounts(&self) -> &[GithubAccount] {
        &self.records.github.accounts
    }

    pub fn github_account(&self, id: &GithubAccountId) -> Option<&GithubAccount> {
        self.records.github.account_at.get(id).map(|&at| &self.records.github.accounts[at])
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

    /// Every app of your own the hub holds the key of, by name, in any case.
    pub fn github_apps(&self) -> &[GithubApp] {
        &self.records.github.apps
    }

    /// The record of the app of your own `id`, until it's removed.
    pub fn github_app(&self, id: GithubAppId) -> Option<&GithubApp> {
        self.records.github.app_at.get(&id).map(|&at| &self.records.github.apps[at])
    }

    /// The login found on each provider's machines that the hub has asked since it started: this machine's first, then
    /// the others' by provider.
    pub fn found_github_logins(&self) -> &[FoundGithubLogin] {
        &self.records.github.found
    }

    /// What a push from the repository `id`'s workspaces would meet, as the hub last checked, or `None` while it hasn't,
    /// and from a hub that doesn't check.
    pub fn github_reach(&self, id: &RepoId) -> Option<&Reach> {
        self.records.github.reach.get(id)
    }
}
