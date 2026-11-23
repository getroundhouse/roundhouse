//! What a fake hub lists of its world, in the order a hub lists it, as `protocol` says for each kind of record, whatever
//! order the world made them in.

use std::cmp::Ordering;

use protocol::model::{DiffComment, HubSettings, Records, Repo, Rev, Session, Workspace, WorkspaceId, WorkspaceStatus};

use super::world::{LoginSupport, World};

impl World {
    /// Everything, as one list.
    pub fn records(&self) -> Records {
        Records {
            repos: self.repos_listed(),
            workspaces: self.workspaces_listed(true),
            sessions: in_order(&self.sessions, Session::oldest_first),
            comments: in_order(self.comments.values().flatten(), DiffComment::oldest_first),
            // As a hub starts out.
            hub_settings: (!self.keeps_no_settings).then(|| HubSettings {
                account_defaults: (self.logins == LoginSupport::Defaults).then(|| self.account_defaults.clone()),
                keep_awake: true,
                allow_asked_in_full_access: Some(false),
                github: self.github.clone(),
                rev: Rev::default(),
            }),
            accounts: self.accounts.clone(),
            // As from a hub that doesn't keep one, since its hub refuses to.
            new_workspace_draft: None,
            github_accounts: self.github_accounts.clone(),
            github_sign_ins: self.github_sign_ins.clone(),
            // As from a hub since Settings' Providers.
            found_github_logins: Vec::new(),
            github_apps: self.github_apps.clone(),
            github_reach: self.github_reach.clone(),
            terminals: self.terminals.iter().map(|(terminal, _)| terminal.clone()).collect(),
            model_lists: self.model_lists.clone(),
            hub_update: self.hub_update.clone(),
            // As from a hub from before observations.
            observations: Vec::new(),
            rev: Rev::default(),
        }
    }

    pub(super) fn repos_listed(&self) -> Vec<Repo> {
        in_order(&self.repos, Repo::by_name)
    }

    /// Every workspace, but for those archived unless `include_archived`.
    pub(super) fn workspaces_listed(&self, include_archived: bool) -> Vec<Workspace> {
        let listed = self.workspaces.iter().filter(|w| include_archived || w.status != WorkspaceStatus::Archived);
        in_order(listed, Workspace::oldest_first)
    }

    /// The chats of `workspace`.
    pub(super) fn sessions_listed(&self, workspace: &WorkspaceId) -> Vec<Session> {
        in_order(self.sessions.iter().filter(|s| &s.workspace_id == workspace), Session::oldest_first)
    }

    /// The comments on `workspace`'s diff.
    pub(super) fn comments_listed(&self, workspace: &WorkspaceId) -> Vec<DiffComment> {
        in_order(self.comments.get(workspace).into_iter().flatten(), DiffComment::oldest_first)
    }
}

fn in_order<'a, T: Clone + 'a>(items: impl IntoIterator<Item = &'a T>, order: impl Fn(&T, &T) -> Ordering) -> Vec<T> {
    let mut listed: Vec<T> = items.into_iter().cloned().collect();
    listed.sort_by(order);
    listed
}
