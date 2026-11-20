//! What a fake hub says to what it's asked of GitHub that isn't a change it refuses: who `gh` on its machine is,
//! connecting that login, looking for an app's installations, and writing an app's manifest.

use protocol::client::github_apps::{GithubAppManifest, GithubAppManifestWrite};
use protocol::client::{ApiError, ClientResponse, ErrorCode};

use super::serve::{Answer, Outcome, read_only};
use super::world::World;

/// Who `gh` on the hub's machine is, as the world says, or that the hub is too old to say.
pub(super) fn gh_login(world: &World) -> Answer {
    match &world.hub_gh_login {
        Some(login) => Ok(Outcome::Reply(ClientResponse::GhLogin(login.clone()), Vec::new())),
        None => Err(ApiError::new(ErrorCode::Unsupported, "an older hub doesn't read gh's login")),
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
