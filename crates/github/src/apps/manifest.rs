use protocol::client::github_apps::{GithubAppManifest, GithubAppManifestWrite};
use protocol::client::{ApiError, ErrorCode};
use serde_json::json;

/// The longest name GitHub takes for an app.
const LONGEST_NAME: usize = 34;

/// Where an app's page on GitHub links to: Roundhouse's own site.
const HOMEPAGE: &str = "https://getroundhouse.dev";

/// What an app of your own may do, as a GitHub App's permissions: what agents use, as the owner decided, in the
/// repositories the person installs it on. The person can give it more on GitHub, on the app's page.
const PERMISSIONS: [(&str, &str); 8] = [
    // What every app has: a repository's name, description and the like.
    ("metadata", "read"),
    // Push, make and delete branches and tags.
    ("contents", "write"),
    // Open, update, review and merge pull requests, as agents do through `gh`.
    ("pull_requests", "write"),
    // Open and comment on issues, which a pull request's conversation is too.
    ("issues", "write"),
    // Without it, GitHub refuses a push that changes `.github/workflows/`.
    ("workflows", "write"),
    // Read a run and its logs, as `gh run` does.
    ("actions", "read"),
    // Read the checks on a commit, as `gh pr checks` and the hub's reads of pull requests do.
    ("checks", "read"),
    // And the statuses on a commit, which those read too.
    ("statuses", "read"),
];

/// The manifest GitHub's manifest flow makes an app of your own from, as [`GithubAppManifestWrite`] asks for it:
/// named `asked.name`, sending the person back to `asked.redirect_url` once they've made it, with
/// [`PERMISSIONS`], and no webhook, since the hub asks GitHub for what it needs rather than being told. Private, as
/// the owner decided, so no one but the account or organization it's made under can install it: the person makes one
/// under each account or organization it's for, at GitHub's form there, which the client opens, and each app has at
/// most one installation, on its owner.
pub fn manifest(asked: &GithubAppManifestWrite) -> Result<GithubAppManifest, ApiError> {
    let GithubAppManifestWrite { name, redirect_url } = asked;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > LONGEST_NAME {
        let message = format!("an app's name has to be from 1 to {LONGEST_NAME} characters long");
        return Err(ApiError::new(ErrorCode::InvalidRequest, message));
    }
    let address = redirect_url.parse::<http::Uri>().ok();
    if !address
        .is_some_and(|address| matches!(address.scheme_str(), Some("http" | "https")) && address.host().is_some())
    {
        return Err(ApiError::new(ErrorCode::InvalidRequest, "where GitHub sends the person back isn't an address"));
    }
    let permissions: serde_json::Map<_, _> =
        PERMISSIONS.into_iter().map(|(permission, access)| (permission.to_string(), json!(access))).collect();
    let manifest = json!({
        "name": name,
        "url": HOMEPAGE,
        "description": "Agents in Roundhouse act as this app, with what it may do, rather than as you.",
        "redirect_url": redirect_url,
        "public": false,
        // Off, so GitHub sends the app nothing. GitHub's docs give a webhook's `url` as required, so it's an address of
        // the kind kept for examples, which nothing is sent to while it's off. Not yet tried on GitHub.
        "hook_attributes": {"url": "https://example.com/unused", "active": false},
        "default_permissions": permissions,
        "default_events": [],
    });
    Ok(GithubAppManifest { manifest })
}
