//! What a fake hub says to what it's asked of GitHub that isn't a change it refuses: who `gh` on its machine is,
//! connecting that login, looking for an app's installations, writing an app's manifest, and what GitHub says of an
//! issue or a pull request, and of every workspace's own pull request, from what its world's [`GithubSays`] holds, in
//! the form it says.

use std::collections::HashMap;

use protocol::client::github::GithubUpdate;
use protocol::client::github_apps::{GithubAppManifest, GithubAppManifestWrite};
use protocol::client::{ApiError, ClientResponse, ErrorCode};
use protocol::model::{
    Level, Observation, Observed, OwnPullRequest, OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary,
    PullRequestDetail, PullRequestFound, Reference, ReferenceAsk, ReferenceRead, ReferenceSaid, RepositoryUnseen, Rev,
    WorkspaceId,
};

use super::serve::{Answer, Outcome, read_only};
use super::world::World;

/// Who `gh` on the hub's machine is, as the world says, or that the hub is too old to say.
pub(super) fn gh_login(world: &World) -> Answer {
    match &world.hub_gh_login {
        Some(login) => Ok(Outcome::reply(ClientResponse::GhLogin(login.clone()))),
        None => Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't read gh's login")),
    }
}

/// Who `gh` and the SSH key on the hub's machine are, as the world says, or refused, as a hub from before it read them.
pub(super) fn machine_logins(world: &World) -> Answer {
    match &world.machine_logins {
        Some(logins) => Ok(Outcome::reply(ClientResponse::MachineLogins(logins.clone()))),
        None => Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't read its machine's logins")),
    }
}

/// Connecting `gh`'s login, which the world refuses as it says, or as a change.
pub(super) fn connect_gh(world: &World) -> Answer {
    Err(world.gh_connect_refused.clone().unwrap_or_else(|| read_only("connect")))
}

/// Looking for where an app is installed, which the world refuses as it says, or as a change.
pub(super) fn look_for_installations(world: &World) -> Answer {
    Err(world.app_look_refused.clone().unwrap_or_else(|| read_only("look")))
}

/// An app's manifest, as a hub writes it, which changes nothing the hub holds, though with only what the app reads of
/// it, its name and where GitHub sends the person back, since nothing here posts it to GitHub.
pub(super) fn manifest(GithubAppManifestWrite { name, redirect_url }: GithubAppManifestWrite) -> ClientResponse {
    let manifest = serde_json::json!({"name": name, "redirect_url": redirect_url, "public": false});
    ClientResponse::GithubAppManifest(GithubAppManifest { manifest })
}

/// What GitHub says of `ask`, as the world has it, which is missing where the world has none.
pub(super) fn reference(world: &World, ask: &ReferenceAsk) -> ReferenceRead {
    let found = world.github_says.references.iter().find(|reference| reference.asked().as_ref() == Some(ask));
    found.map_or(ReferenceRead::Missing, |reference| ReferenceRead::Found { reference: Box::new(reference.clone()) })
}

/// What GitHub says of every workspace's own pull request, as a read would find it, from the world's, as a hub sends it
/// at its level. One whose repository GitHub showed none of is what was known before, which is the world's pull
/// request, if it has one, and otherwise not looked for.
pub(super) fn own_pull_requests(world: &World) -> impl Iterator<Item = GithubUpdate> + '_ {
    world.workspaces_listed(true).into_iter().map(|workspace| {
        let id = workspace.id;
        let own = world.github_says.pull_requests.get(&id).cloned();
        let unseen = world.github_says.unseen.get(&id).cloned();
        let found = match &unseen {
            Some(_) => own.map(|own| PullRequestFound { pull_request: Some(own) }),
            None => Some(PullRequestFound { pull_request: own }),
        };
        GithubUpdate::PullRequest { workspace: id, found, unseen }
    })
}

/// What GitHub says of a fake hub's world, as the hub would have read it, and the form the hub sends it in.
#[derive(Default)]
pub struct GithubSays {
    /// Each workspace's own pull request, as the hub would have read it.
    pub pull_requests: HashMap<WorkspaceId, OwnPullRequest>,
    /// The workspaces whose repository GitHub showed the default none of, as the hub last read it.
    pub unseen: HashMap<WorkspaceId, RepositoryUnseen>,
    /// The issues and pull requests on GitHub that chats link to, as the hub would have looked them up.
    pub references: Vec<Reference>,
    /// Which form the hub sends it in, to a start that reads it as observations, as a hub of this build does unless a
    /// test stands in for an older one.
    pub sends: GithubSends,
}

/// Which form a hub sends what GitHub says in, to a start of `Subscribe` that reads it as observations. A start that
/// doesn't is sent `HubFrame::Github` by every one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GithubSends {
    /// As observations at every level, as a hub of this build does.
    #[default]
    Observations,
    /// As observations of a chip alone, and a workspace's pull request as `HubFrame::Github` at the hub's level, as a
    /// hub from before it sent a workspace's pull request as observations does.
    ChipsObserved,
    /// As `HubFrame::Github` at every level, as a hub from before `github_observed` does, which ignores it.
    Updates,
}

/// Whether a start at `level` that says `github_observed` is sent what GitHub says there as observations, as the
/// world's hub would send them, rather than as `HubFrame::Github`.
pub(super) fn observed(world: &World, github_observed: bool, level: &Level) -> bool {
    github_observed
        && match world.github_says.sends {
            GithubSends::Observations => true,
            GithubSends::ChipsObserved => matches!(level, Level::Reference { .. }),
            GithubSends::Updates => false,
        }
}

/// How every workspace's own pull request stands, as the hub's level sends it as observations, from the same as
/// [`own_pull_requests`] says. One whose repository GitHub showed none of, and that has no pull request known from
/// before, has none, as not looked for.
pub(super) fn own_observed(world: &World) -> Vec<Observation> {
    let stands = |id: WorkspaceId| {
        let own = world.github_says.pull_requests.get(&id);
        if own.is_none() && world.github_says.unseen.contains_key(&id) {
            return None;
        }
        let value = OwnPullRequestSaid { pull_request: own.map(OwnSummary::of) };
        Some(Observation::WorkspacePullRequest { workspace_id: id, pull_request: current(value), rev: Rev::default() })
    };
    world.workspaces_listed(true).into_iter().filter_map(|workspace| stands(workspace.id)).collect()
}

/// What a workspace's level sends as observations of its pull request: its checks, history and stack, and whether
/// GitHub showed its repository, from the same as [`own_pull_requests`] says.
pub(super) fn workspace_observed(world: &World, id: &WorkspaceId) -> Vec<Observation> {
    let own = world.github_says.pull_requests.get(id);
    let unseen = world.github_says.unseen.get(id).cloned();
    let detail = (own.is_some() || unseen.is_none()).then(|| Observation::WorkspacePullRequestDetail {
        workspace_id: id.clone(),
        detail: current(OwnPullRequestDetail { pull_request: own.map(|own| PullRequestDetail::of(&own.pull_request)) }),
        rev: Rev::default(),
    });
    let unseen = unseen.map(|unseen| Observation::WorkspaceRepositoryUnseen {
        workspace_id: id.clone(),
        unseen: current(Some(unseen)),
        rev: Rev::default(),
    });
    detail.into_iter().chain(unseen).collect()
}

/// What a chip's level sends as the observation of `ask`, which is missing where the world has none.
pub(super) fn reference_observed(world: &World, ask: &ReferenceAsk) -> Vec<Observation> {
    let found = world.github_says.references.iter().find(|reference| reference.asked().as_ref() == Some(ask));
    let said = ReferenceSaid::of(found);
    said.map(|said| Observation::Reference { reference: ask.clone(), said: current(said), rev: Rev::default() })
        .into_iter()
        .collect()
}

/// `value`, as kept up to date, as what a client follows is.
fn current<T>(value: T) -> Observed<T> {
    Observed::Current { value }
}
