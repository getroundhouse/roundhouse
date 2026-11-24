//! What a fake hub says to what it's asked of GitHub that isn't a change it refuses: who `gh` on its machine is,
//! connecting that login, looking for an app's installations, writing an app's manifest, and what GitHub says of an
//! issue or a pull request, and of every workspace's own pull request, from what its world's [`GithubSays`] holds.

use std::collections::HashMap;

use protocol::client::github_apps::{GithubAppManifest, GithubAppManifestWrite};
use protocol::client::{ApiError, ClientResponse, ErrorCode};
use protocol::model::{
    Observation, Observed, OwnPullRequest, OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary, PullRequestDetail,
    Reference, ReferenceAsk, ReferenceRead, ReferenceSaid, RepositoryUnseen, WorkspaceId,
};

use super::revisions::MADE;
use super::serve::{Answer, Outcome, read_only};
use super::world::World;

/// Who `gh` on the hub's machine is, as the world says, or that the hub couldn't say.
pub(super) fn gh_login(world: &World) -> Answer {
    match &world.hub_gh_login {
        Some(login) => Ok(Outcome::reply(ClientResponse::GhLogin(login.clone()))),
        None => Err(ApiError::new(ErrorCode::Internal, "a fake hub's world says nothing of gh's login")),
    }
}

/// Who `gh` and the SSH key on the hub's machine are, as the world says, or that the hub couldn't say.
pub(super) fn machine_logins(world: &World) -> Answer {
    match &world.machine_logins {
        Some(logins) => Ok(Outcome::reply(ClientResponse::MachineLogins(logins.clone()))),
        None => Err(ApiError::new(ErrorCode::Internal, "a fake hub's world says nothing of its machine's logins")),
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

/// What GitHub says of a fake hub's world, as the hub would have read it.
#[derive(Default)]
pub struct GithubSays {
    /// Each workspace's own pull request, as the hub would have read it.
    pub pull_requests: HashMap<WorkspaceId, OwnPullRequest>,
    /// The workspaces whose repository GitHub showed the default none of, as the hub last read it.
    pub unseen: HashMap<WorkspaceId, RepositoryUnseen>,
    /// The issues and pull requests on GitHub that chats link to, as the hub would have looked them up.
    pub references: Vec<Reference>,
}

/// How every workspace's own pull request stands, as the hub's level sends it as observations. One whose repository
/// GitHub showed none of, and that has no pull request known from before, has none, as not looked for.
pub(super) fn own_observed(world: &World) -> Vec<Observation> {
    let stands = |id: WorkspaceId| {
        let own = world.github_says.pull_requests.get(&id);
        if own.is_none() && world.github_says.unseen.contains_key(&id) {
            return None;
        }
        let value = OwnPullRequestSaid { pull_request: own.map(OwnSummary::of) };
        Some(Observation::WorkspacePullRequest { workspace_id: id, pull_request: current(value), rev: MADE })
    };
    world.workspaces_listed(true).into_iter().filter_map(|workspace| stands(workspace.id)).collect()
}

/// What a workspace's level sends as observations of its pull request: its checks, history and stack, and whether
/// GitHub showed its repository.
pub(super) fn workspace_observed(world: &World, id: &WorkspaceId) -> Vec<Observation> {
    let own = world.github_says.pull_requests.get(id);
    let unseen = world.github_says.unseen.get(id).cloned();
    let detail = (own.is_some() || unseen.is_none()).then(|| Observation::WorkspacePullRequestDetail {
        workspace_id: id.clone(),
        detail: current(OwnPullRequestDetail { pull_request: own.map(|own| PullRequestDetail::of(&own.pull_request)) }),
        rev: MADE,
    });
    let unseen = unseen.map(|unseen| Observation::WorkspaceRepositoryUnseen {
        workspace_id: id.clone(),
        unseen: current(Some(unseen)),
        rev: MADE,
    });
    detail.into_iter().chain(unseen).collect()
}

/// What a chip's level sends as the observation of `ask`, which is missing where the world has none.
pub(super) fn reference_observed(world: &World, ask: &ReferenceAsk) -> Vec<Observation> {
    let found = world.github_says.references.iter().find(|reference| reference.asked().as_ref() == Some(ask));
    let said = ReferenceSaid::of(found);
    said.map(|said| Observation::Reference { reference: ask.clone(), said: current(said), rev: MADE })
        .into_iter()
        .collect()
}

/// `value`, as kept up to date, as what a client follows is.
fn current<T>(value: T) -> Observed<T> {
    Observed::Current { value }
}
