//! The pages on GitHub of an app of your own that the desktop sends the person to, from Settings, where it's made and
//! listed, and from a workspace whose repository it can't push to, so both send them to the same page.

use protocol::model::GithubAppSlug;

/// Where GitHub installs the app called `slug` from, letting the person choose its repositories, and, where it's
/// installed already, change which it reaches.
pub(crate) fn install_page(slug: &GithubAppSlug) -> String {
    format!("https://github.com/apps/{slug}/installations/new")
}
