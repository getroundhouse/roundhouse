//! What a fake hub says to what it's asked of GitHub that isn't a change it refuses: who `gh` on its machine is,
//! connecting that login, looking for an app's installations, writing an app's manifest, and what GitHub says of what
//! a client shows.

use protocol::client::github::{GithubSubscribe, GithubUpdate};
use protocol::client::github_apps::{GithubAppManifest, GithubAppManifestWrite};
use protocol::client::{ApiError, ClientResponse, ErrorCode, HubFrame};
use protocol::model::{PullRequestFound, ReferenceAsk, ReferenceRead};

use super::serve::{Answer, Outcome, read_only};
use super::world::World;

/// Who `gh` on the hub's machine is, as the world says, or that the hub is too old to say.
pub(super) fn gh_login(world: &World) -> Answer {
    match &world.hub_gh_login {
        Some(login) => Ok(Outcome::Reply(ClientResponse::GhLogin(login.clone()), Vec::new())),
        None => Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't read gh's login")),
    }
}

/// Who `gh` and the SSH key on the hub's machine are, as the world says, or refused, as a hub from before it read them.
pub(super) fn machine_logins(world: &World) -> Answer {
    match &world.machine_logins {
        Some(logins) => Ok(Outcome::Reply(ClientResponse::MachineLogins(logins.clone()), Vec::new())),
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
    let found = world.references.iter().find(|reference| reference.asked().as_ref() == Some(ask));
    found.map_or(ReferenceRead::Missing, |reference| ReferenceRead::Found { reference: Box::new(reference.clone()) })
}

/// What a hub sends at once of what `subscribe` shows, since nothing here changes after: each chip as the world has
/// it, and each workspace's pull request as a read would find it, from the world's.
pub(super) fn shown(world: &World, subscribe: GithubSubscribe) -> Vec<HubFrame> {
    let GithubSubscribe { subscription_id, shown } = subscribe;
    world.github_shown.lock().push(shown.clone());
    let references = (shown.references.iter())
        .map(|ask| GithubUpdate::Reference { reference: ask.clone(), read: reference(world, ask) });
    let pull_requests =
        shown.workspaces.iter().filter(|id| world.workspaces.iter().any(|workspace| workspace.id == **id)).map(|id| {
            let found = PullRequestFound { pull_request: world.pull_requests.get(id).cloned() };
            GithubUpdate::PullRequest { workspace: id.clone(), found: Some(found) }
        });
    references.chain(pull_requests).map(|update| HubFrame::Github { subscription_id, update }).collect()
}
