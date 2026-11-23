//! The pages on GitHub of an app of your own that the desktop sends the person to, from Settings, where it's made and
//! listed, and from a workspace whose repository it can't push to, so both send them to the same page.

use protocol::model::{GithubAppSlug, GithubRepositoryName, GithubUsername};

/// Where GitHub installs the app called `slug` from, letting the person choose its repositories, and, where it's
/// installed already, change which it reaches.
pub(crate) fn install_page(slug: &GithubAppSlug) -> String {
    format!("https://github.com/apps/{slug}/installations/new")
}

/// Whether `repository` is of the owner an app is installed `on`, as GitHub compares names, without case. An app of
/// your own is installed only on its owner, so it reaches no other owner's repositories.
pub(crate) fn installed_on_owner(repository: &GithubRepositoryName, on: &GithubUsername) -> bool {
    repository.owner().eq_ignore_ascii_case(on.as_str())
}
